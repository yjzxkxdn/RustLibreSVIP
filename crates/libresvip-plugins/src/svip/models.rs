//! Port of `libresvip.plugins.svip.models`.
//!
//! `singers.json` is embedded verbatim (the Python plugin reads it from its
//! package resources); the bidicts for reverb presets and note head tags are
//! plain tables.

/// `opensvip_singers.singers` — `(singer id, singer name)`, file order.
pub static SINGERS: [(&str, &str); 81] = [
    ("XiaoIce", "小冰"),
    ("M3", "杨以凡"),
    ("M13", "陈子渝"),
    ("M803", "挚彬同学"),
    ("M806", "雲宇光"),
    ("M810", "辉宇·星"),
    ("M812", "陆思川"),
    ("M820", "严清语"),
    ("M822", "泰伦"),
    ("M825", "雨宫晓"),
    ("M832", "埃洛"),
    ("M835", "小格"),
    ("M841", "五世百米"),
    ("M844", "翱天"),
    ("M848", "洛希"),
    ("M850", "玖辰"),
    ("M853", "九棠Twi"),
    ("M905", "孙枫"),
    ("M906", "袁率"),
    ("M907", "稻光"),
    ("M909", "豪叶"),
    ("M910", "冰冰火"),
    ("M911", "云灏"),
    ("M916", "沐凌弦"),
    ("M917", "白鬼月"),
    ("M918", "枫月"),
    ("M932", "遂安"),
    ("M935", "十瑚"),
    ("M937", "墨讱_DΞfαuΙΓ"),
    ("M938", "柯泰璃"),
    ("M946", "殷子之瀛"),
    ("M958", "霖漓"),
    ("M959", "雾怜之"),
    ("M966", "墨皑"),
    ("M972", "云野"),
    ("F10", "陈水若"),
    ("F11", "何畅"),
    ("F801", "徐爱颜"),
    ("F802", "荼鸢"),
    ("F809", "海莉"),
    ("F813", "蔷芜"),
    ("F814", "幻神·米丝"),
    ("F819", "方念"),
    ("F827", "大宝剑"),
    ("F828", "舞语"),
    ("F830", "绮萱"),
    ("F836", "果妹"),
    ("F838", "小夜"),
    ("F840", "Arya"),
    ("F842", "曦和 律"),
    ("F843", "小傻"),
    ("F845", "瑶(YAO)"),
    ("F846", "月柬依"),
    ("F849", "娜芙(Nerv)"),
    ("F851", "楚瓷"),
    ("F852", "时久琉"),
    ("F854", "叶妮娅(Еня)"),
    ("F901", "麦笛奈"),
    ("F903", "尘霜"),
    ("F908", "虞宁xs"),
    ("F912", "寺倾花"),
    ("F914", "时枝"),
    ("F922", "艾尔法"),
    ("F923", "狐狸座"),
    ("F924", "华智冰"),
    ("F931", "诗芸"),
    ("F933", "小珞"),
    ("F934", "潘"),
    ("F939", "月月"),
    ("F940", "溯羽"),
    ("F941", "君凝华"),
    ("F942", "枫聆月Lyria"),
    ("F944", "破晓 AkiRA"),
    ("F949", "雨令"),
    ("F961", "炉柚RoYo"),
    ("F964", "桃灼"),
    ("F965", "奈月"),
    ("F968", "冰雪唐"),
    ("F971", "安可"),
    ("F974", "十叶剡"),
    ("0", "观雨"),
];

/// `prefix_match(r"[FM]\d+", value) is not None` — `[FM]` followed by at least
/// one ASCII digit at the start of the string.
fn matches_singer_id(value: &str) -> bool {
    let mut characters = value.chars();
    match characters.next() {
        Some('F' | 'M') => {}
        _ => return false,
    }
    let mut digits = 0usize;
    for character in characters {
        if !character.is_ascii_digit() {
            return false;
        }
        digits += 1;
    }
    digits > 0
}

/// `prefix_match(r"\$\([FM]\d+\)", value) is not None` — note that Python uses
/// `re.match`, so trailing garbage after the closing parenthesis still matches.
fn matches_open_svip_singer(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("$(") else {
        return false;
    };
    let mut characters = rest.chars();
    match characters.next() {
        Some('F' | 'M') => {}
        _ => return false,
    }
    let mut digits = 0usize;
    for character in characters {
        if character.is_ascii_digit() {
            digits += 1;
            continue;
        }
        return character == ')' && digits > 0;
    }
    false
}

/// `opensvip_singers.get_name(id_)`.
pub fn singer_name(id: &str) -> String {
    if let Some((_, name)) = SINGERS.iter().find(|(singer_id, _)| *singer_id == id) {
        return (*name).to_owned();
    }
    if matches_singer_id(id) {
        format!("$({id})")
    } else {
        String::new()
    }
}

/// `opensvip_singers.get_id(name)`.
pub fn singer_id(name: &str) -> String {
    if let Some((id, _)) = SINGERS.iter().find(|(_, singer_name)| *singer_name == name) {
        return (*id).to_owned();
    }
    if matches_open_svip_singer(name) {
        // Python slices `name[2:-1]` after the (unanchored) match.
        let mut characters: Vec<char> = name.chars().collect();
        if characters.len() >= 3 {
            characters.truncate(characters.len() - 1);
            return characters[2..].iter().collect();
        }
    }
    String::new()
}

/// `svip_reverb_presets` — `(Chinese name, XSReverbPresetEnum value)`.
pub static REVERB_PRESETS: [(&str, i32); 8] = [
    ("干声", -1),
    ("浮光", 0),
    ("午后", 1),
    ("月光", 3),
    ("水晶", 5),
    ("汽水", 7),
    ("夜莺", 9),
    ("大梦", 18),
];

/// `svip_reverb_presets.inverse.get(value)`.
pub fn reverb_name(value: i32) -> Option<&'static str> {
    REVERB_PRESETS
        .iter()
        .find(|(_, preset)| *preset == value)
        .map(|(name, _)| *name)
}

/// `svip_reverb_presets.get(name, XSReverbPresetEnum.NONE)`.
pub fn reverb_value(name: &str) -> i32 {
    REVERB_PRESETS
        .iter()
        .find(|(preset_name, _)| *preset_name == name)
        .map(|(_, value)| *value)
        .unwrap_or(-1)
}

/// `svip_note_head_tags.inverse.get(value)` — `{NoTag: None, "0": SilTag,
/// "V": SpTag}`.
pub fn note_head_tag_name(value: i32) -> Option<&'static str> {
    match value {
        1 => Some("0"),
        2 => Some("V"),
        _ => None,
    }
}

/// `svip_note_head_tags.get(head_tag, XSNoteHeadTagEnum.NoTag)`.
pub fn note_head_tag_value(tag: Option<&str>) -> i32 {
    match tag {
        Some("0") => 1,
        Some("V") => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singer_name_follows_python() {
        assert_eq!(singer_name("F10"), "陈水若");
        assert_eq!(singer_name("F12345"), "$(F12345)");
        assert_eq!(singer_name("M0"), "$(M0)");
        assert_eq!(singer_name("Singer"), "");
        assert_eq!(singer_name("F"), "");
        assert_eq!(singer_name("F1x"), "");
    }

    #[test]
    fn singer_id_follows_python() {
        assert_eq!(singer_id("陈水若"), "F10");
        assert_eq!(singer_id("$(F10)"), "F10");
        // Python's `re.match` is not anchored, then slices `name[2:-1]`.
        assert_eq!(singer_id("$(F10)x"), "F10)");
        assert_eq!(singer_id("$(M3)!"), "M3)");
        assert_eq!(singer_id("$(X10)"), "");
        assert_eq!(singer_id("Singer"), "");
    }

    #[test]
    fn preset_tables_are_bijective() {
        for (name, value) in REVERB_PRESETS {
            assert_eq!(reverb_name(value), Some(name));
            assert_eq!(reverb_value(name), value);
        }
        assert_eq!(reverb_name(2), None);
        assert_eq!(reverb_value(""), -1);
        assert_eq!(note_head_tag_name(0), None);
        assert_eq!(note_head_tag_name(1), Some("0"));
        assert_eq!(note_head_tag_name(2), Some("V"));
        assert_eq!(note_head_tag_name(3), None);
        assert_eq!(note_head_tag_value(None), 0);
        assert_eq!(note_head_tag_value(Some("0")), 1);
        assert_eq!(note_head_tag_value(Some("V")), 2);
        assert_eq!(note_head_tag_value(Some("x")), 0);
    }
}
