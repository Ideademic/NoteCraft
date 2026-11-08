//! Proofing languages in `.one` files: MS-ONE stores a paragraph's language as a Windows
//! language code identifier (LanguageID, RichEditTextLangID), the values [MS-LCID] lists. NoteCraft
//! keeps BCP 47 tags; this maps the languages it offers and the common others both ways.

/// (LCID, BCP 47 tag), from [MS-LCID].
const TABLE: &[(u32, &str)] = &[
    (0x0409, "en-US"),
    (0x0809, "en-GB"),
    (0x0C09, "en-AU"),
    (0x1009, "en-CA"),
    (0x1409, "en-NZ"),
    (0x1809, "en-IE"),
    (0x1C09, "en-ZA"),
    (0x4009, "en-IN"),
    (0x0402, "bg-BG"),
    (0x0405, "cs-CZ"),
    (0x0406, "da-DK"),
    (0x0407, "de-DE"),
    (0x0807, "de-CH"),
    (0x0C07, "de-AT"),
    (0x0408, "el-GR"),
    (0x0C0A, "es-ES"),
    (0x080A, "es-MX"),
    (0x580A, "es-419"),
    (0x380A, "es-UY"),
    (0x2C0A, "es-AR"),
    (0x040B, "fi-FI"),
    (0x040C, "fr-FR"),
    (0x0C0C, "fr-CA"),
    (0x080C, "fr-BE"),
    (0x100C, "fr-CH"),
    (0x040D, "he-IL"),
    (0x041A, "hr-HR"),
    (0x040E, "hu-HU"),
    (0x0421, "id-ID"),
    (0x0410, "it-IT"),
    (0x0411, "ja-JP"),
    (0x0412, "ko-KR"),
    (0x0413, "nl-NL"),
    (0x0813, "nl-BE"),
    (0x0414, "nb-NO"),
    (0x0415, "pl-PL"),
    (0x0416, "pt-BR"),
    (0x0816, "pt-PT"),
    (0x0418, "ro-RO"),
    (0x0419, "ru-RU"),
    (0x041B, "sk-SK"),
    (0x0424, "sl-SI"),
    (0x041D, "sv-SE"),
    (0x041F, "tr-TR"),
    (0x0422, "uk-UA"),
    (0x0401, "ar-SA"),
    (0x0804, "zh-CN"),
    (0x0404, "zh-TW"),
];

/// The LCID of a language tag (`en-GB`, `en_gb`; a bare `pl` takes its first region here).
pub fn from_tag(tag: &str) -> Option<u32> {
    let t = tag.trim().replace('_', "-");
    TABLE
        .iter()
        .find(|(_, k)| k.eq_ignore_ascii_case(&t))
        .or_else(|| TABLE.iter().find(|(_, k)| k.split('-').next().is_some_and(|p| p.eq_ignore_ascii_case(&t))))
        .map(|(id, _)| *id)
}

/// The language tag of an LCID NoteCraft knows.
pub fn to_tag(lcid: u32) -> Option<&'static str> {
    TABLE.iter().find(|(id, _)| *id == lcid).map(|(_, t)| *t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_ways() {
        for (id, tag) in TABLE {
            assert_eq!(from_tag(tag), Some(*id));
            assert_eq!(to_tag(*id), Some(*tag));
        }
        assert_eq!(from_tag("en_gb"), Some(0x0809));
        assert_eq!(from_tag("pl"), Some(0x0415));
        assert_eq!(from_tag("zxx"), None);
        assert_eq!(to_tag(0xFFFF), None);
    }
}
