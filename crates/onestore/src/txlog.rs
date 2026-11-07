//! The transaction log ([MS-ONESTORE] §2.3.3): which file node list lengths are committed.
//!
//! Only the first `cTransactionsInLog` transactions count; entries after them, and the file
//! nodes they added, are ignored.

use std::collections::HashMap;

use crate::bytes::{Reader, WriteLe, slice};
use crate::header::{Fcr, FileKind};
use crate::{Error, Result};

/// Size of one TransactionEntry.
const ENTRY_SIZE: usize = 8;
/// The sentinel `srcID` that ends a transaction.
const SENTINEL: u32 = 1;
/// Most log fragments followed (hostile files can loop).
const MAX_FRAGMENTS: usize = 100_000;

struct State {
    log: TransactionLog,
    pending: Vec<(u32, u32)>,
    done: u32,
    want: u32,
}

impl State {
    fn apply(&mut self, r: &mut Reader<'_>) -> Result<()> {
        while r.remaining() >= ENTRY_SIZE && self.done < self.want {
            let src = r.u32()?;
            let switch = r.u32()?;
            if src == SENTINEL {
                for (list, count) in self.pending.drain(..) {
                    self.log.counts.insert(list, count);
                }
                self.done += 1;
            } else {
                self.pending.push((src, switch));
            }
        }
        Ok(())
    }
}

/// Committed node counts by FileNodeListID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransactionLog {
    pub counts: HashMap<u32, u32>,
}

impl TransactionLog {
    /// Reads the log starting at `first`, applying `transactions` transactions.
    pub fn read(file: &[u8], first: Fcr, transactions: u32) -> Result<TransactionLog> {
        let mut st = State { log: TransactionLog::default(), pending: Vec::new(), done: 0, want: transactions };
        let mut fcr = first;
        let mut fragments = 0usize;
        while st.done < transactions {
            fragments += 1;
            if fragments > MAX_FRAGMENTS || !fcr.is_some() {
                return Err(Error::Corrupt("transaction log: ends before the committed transactions".into()));
            }
            let frag = slice(file, fcr.stp, fcr.cb, "transaction log fragment")?;
            let table_len = frag.len().checked_sub(12).ok_or_else(|| Error::Corrupt("transaction log fragment too small".into()))?;
            st.apply(&mut Reader::new(frag.get(..table_len).unwrap_or_default()))?;
            if st.done >= transactions {
                break;
            }
            let tail = frag.get(table_len..).unwrap_or_default();
            let next = Fcr::read64x32(&mut Reader::new(tail))?;
            if next.is_some() && slice(file, next.stp, next.cb, "next transaction log fragment").is_ok() {
                fcr = next;
                continue;
            }
            // `nextFragment` is optional in the fragment holding the last transaction: the tail
            // may be entries instead.
            st.apply(&mut Reader::new(tail))?;
            fcr = Fcr::NIL;
        }
        Ok(st.log)
    }

    /// The committed node count of a list, if a transaction mentioned it.
    pub fn count(&self, list_id: u32) -> Option<u32> {
        self.counts.get(&list_id).copied()
    }
}

/// One transaction's log fragment: an entry per list (id, node count) in order, the sentinel
/// (whose value is the CRC of the entries), and a nil `nextFragment`.
pub fn write_fragment(kind: FileKind, lists: &[(u32, u32)]) -> Vec<u8> {
    let mut entries = Vec::with_capacity(lists.len() * ENTRY_SIZE);
    for &(id, count) in lists {
        entries.put_u32(id);
        entries.put_u32(count);
    }
    let crc = kind.crc(&entries);
    let mut out = entries;
    out.put_u32(SENTINEL);
    out.put_u32(crc);
    Fcr::NIL.write64x32(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_one_transaction() {
        let frag = write_fragment(FileKind::Section, &[(0x10, 4), (0x11, 2)]);
        let mut file = vec![0u8; 16];
        file.extend_from_slice(&frag);
        let log = TransactionLog::read(&file, Fcr::new(16, frag.len() as u64), 1).unwrap();
        assert_eq!(log.count(0x10), Some(4));
        assert_eq!(log.count(0x11), Some(2));
        assert_eq!(log.count(0x12), None);
    }

    #[test]
    fn uncommitted_entries_are_ignored() {
        let mut frag = Vec::new();
        for (a, b) in [(0x10u32, 1u32), (SENTINEL, 0), (0x10, 9), (SENTINEL, 0)] {
            frag.put_u32(a);
            frag.put_u32(b);
        }
        Fcr::NIL.write64x32(&mut frag);
        let log = TransactionLog::read(&frag, Fcr::new(0, frag.len() as u64), 1).unwrap();
        assert_eq!(log.count(0x10), Some(1));
        let log = TransactionLog::read(&frag, Fcr::new(0, frag.len() as u64), 2).unwrap();
        assert_eq!(log.count(0x10), Some(9));
        assert!(TransactionLog::read(&frag, Fcr::new(0, frag.len() as u64), 3).is_err());
    }
}
