//! Page versions as [MS-ONE] version history pages (§2.1.17, §2.1.18).
//!
//! MS-ONE keeps a page's versions inside the page's own object space, as revisions in other
//! contexts ([MS-ONESTORE] `RevisionManifestStart7FND.gctxid`):
//!
//! - the page manifest's VersionHistoryGraphSpaceContextNodes names the version history object
//!   space's context, always {7111497f-1b6b-4209-9491-c98b04cf4c5a}, 1;
//! - that revision's content root is a jcidVersionHistoryContent and its metadata root a
//!   jcidVersionHistoryMetaData (OneNote's own files carry this pair on every page, with no
//!   versions under it);
//! - each version is a jcidVersionProxy (LastModifiedTimeStamp, CreationTimeStamp) pointing at
//!   another context whose revision is the page as it was: a whole PageObjectSpace.
//!
//! MS-ONE names the proxy's link to its version (VersionContextNodes) without numbering it, and
//! a proxy in the documented ElementChildNodes without that link would be broken for OneNote. So
//! NoteCraft leaves ElementChildNodes unset, lists its proxies in [`prop::NC_VERSION_PROXIES`],
//! and links each to its version with [`prop::NC_VERSION_CONTEXT`]: OneNote sees a page without
//! versions and, per MS-ONE §2.1.12, keeps the private properties. The version pages themselves
//! are ordinary page object space revisions, written by the same code as the page.
//!
//! Reading also picks up version pages it can't link (other contexts whose content root is a
//! page manifest), dated by their own last change.

use std::collections::HashSet;

use notecraft_doc::history::MAX_VERSIONS_PER_PAGE;
use notecraft_doc::{Guid, Page, PageVersion};
use notecraft_onestore::{ContextRevision, ExtendedGuid, Object, ObjectSpace, PropertySet, PropertyValue};

use crate::ids::{SCHEMA_REVISION, VERSION_HISTORY_CONTEXT, jcid, prop, role};
use crate::util::{from_filetime, to_filetime, to_time32};
use crate::{Result, read, write};

/// The version history object space's context.
pub fn history_context() -> ExtendedGuid {
    ExtendedGuid::new(VERSION_HISTORY_CONTEXT, 1)
}

/// Adds `page`'s versions to its object space: one context revision per version, the version
/// history revision listing them, and the manifest's link to it. Nothing for a page without
/// versions.
pub(crate) fn attach(space: &mut ObjectSpace, manifest: ExtendedGuid, page: &Page) -> Result<()> {
    if page.versions.is_empty() {
        return Ok(());
    }
    let vh = history_context();
    let mut proxies = Vec::new();
    let mut objects = Vec::new();
    let mut seen = HashSet::new();
    // Newest last; at most what the model keeps (proxy ids count up from 4 within one GUID).
    let skip = page.versions.len().saturating_sub(MAX_VERSIONS_PER_PAGE);
    for v in page.versions.iter().skip(skip) {
        let ctx = ExtendedGuid::from_guid(if v.id.is_nil() { Guid::new() } else { v.id }, 1);
        if ctx == vh || !seen.insert(ctx) {
            continue;
        }
        let (vspace, _) = write::page_space(&v.page)?;
        space.contexts.push(ContextRevision { context: ctx, roots: vspace.roots, objects: vspace.objects });
        let id = ExtendedGuid::new(VERSION_HISTORY_CONTEXT, 4 + proxies.len() as u32);
        let mut p = PropertySet::default();
        p.push(prop::NC_VERSION_CONTEXT, PropertyValue::Ids(vec![ctx]))
            .push(prop::LAST_MODIFIED_TIME_STAMP, PropertyValue::U64(to_filetime(v.saved)))
            .push(prop::CREATION_TIME_STAMP, PropertyValue::U32(to_time32(v.saved)));
        objects.push(Object::new(id, jcid::VERSION_PROXY, p));
        proxies.push(id);
    }
    let content = ExtendedGuid::new(VERSION_HISTORY_CONTEXT, 2);
    let meta = ExtendedGuid::new(VERSION_HISTORY_CONTEXT, 3);
    let mut c = PropertySet::default();
    c.push(prop::NC_VERSION_PROXIES, PropertyValue::Ids(proxies));
    let mut m = PropertySet::default();
    m.push(prop::SCHEMA_REVISION_IN_ORDER_TO_READ, PropertyValue::U32(SCHEMA_REVISION))
        .push(prop::SCHEMA_REVISION_IN_ORDER_TO_WRITE, PropertyValue::U32(SCHEMA_REVISION));
    objects.insert(0, Object::new(meta, jcid::VERSION_HISTORY_METADATA, m));
    objects.insert(0, Object::new(content, jcid::VERSION_HISTORY_CONTENT, c));
    space.contexts.push(ContextRevision { context: vh, roots: vec![(role::CONTENT, content), (role::METADATA, meta)], objects });
    if let Some(o) = space.objects.iter_mut().find(|o| o.id == manifest) {
        o.props.push(prop::VERSION_HISTORY_GRAPH_SPACE_CONTEXT_NODES, PropertyValue::Ids(vec![vh]));
    }
    Ok(())
}

/// The versions stored in a page's object space, oldest first. `manifest` is the page's
/// manifest object.
pub(crate) fn read(r: &mut read::Reader, space: &ObjectSpace, manifest: &Object) -> Vec<PageVersion> {
    if space.contexts.is_empty() {
        return Vec::new();
    }
    let vh = manifest.props.ids(prop::VERSION_HISTORY_GRAPH_SPACE_CONTEXT_NODES).first().copied().unwrap_or_else(history_context);
    let mut used: HashSet<ExtendedGuid> = HashSet::new();
    used.insert(vh);
    let mut out = Vec::new();
    if let Some(history) = space.context(vh) {
        let index = history.index();
        let get = |id: &ExtendedGuid| index.get(id).and_then(|&i| history.objects.get(i));
        let content = history.root(role::CONTENT).and_then(|id| get(&id)).filter(|o| o.jcid == jcid::VERSION_HISTORY_CONTENT);
        for proxy in content.map(|c| c.props.ids(prop::NC_VERSION_PROXIES)).unwrap_or_default().iter().filter_map(get) {
            if proxy.jcid != jcid::VERSION_PROXY {
                continue;
            }
            let Some(ctx) = proxy.props.ids(prop::NC_VERSION_CONTEXT).first().copied() else { continue };
            if !used.insert(ctx) {
                continue;
            }
            let Some(page) = space.context(ctx).and_then(|c| version_page(r, space, c)) else { continue };
            let saved = proxy.props.uint(prop::LAST_MODIFIED_TIME_STAMP).and_then(from_filetime).unwrap_or(page.modified);
            out.push(PageVersion { id: Guid(ctx.guid), saved, page: std::sync::Arc::new(page) });
        }
    }
    // Version pages NoteCraft can't link to a proxy (OneNote's own).
    for c in &space.contexts {
        if out.len() >= MAX_VERSIONS_PER_PAGE * 4 {
            break;
        }
        if used.contains(&c.context) {
            continue;
        }
        let is_page = c.root(role::CONTENT).and_then(|id| c.objects.iter().find(|o| o.id == id)).is_some_and(|o| o.jcid == jcid::PAGE_MANIFEST_NODE);
        if !is_page {
            continue;
        }
        if let Some(page) = version_page(r, space, c) {
            out.push(PageVersion { id: Guid(c.context.guid), saved: page.modified, page: std::sync::Arc::new(page) });
        }
    }
    notecraft_doc::history::trim_versions(&mut out);
    out
}

/// A version's page: the context revision read like a page object space.
fn version_page(r: &mut read::Reader, space: &ObjectSpace, c: &ContextRevision) -> Option<Page> {
    let as_space = ObjectSpace { id: space.id, roots: c.roots.clone(), objects: c.objects.clone(), contexts: Vec::new() };
    let mut page = r.page(&as_space)?;
    page.versions.clear();
    Some(page)
}
