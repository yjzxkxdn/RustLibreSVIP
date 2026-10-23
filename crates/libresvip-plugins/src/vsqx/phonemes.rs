//! Port of the lyric→phoneme helpers used by the Vocaloid plugins:
//! `libresvip.core.lyric_phoneme.chinese.vocaloid_xsampa`,
//! `libresvip.core.lyric_phoneme.japanese` (romaji) and
//! `libresvip.core.lyric_phoneme.japanese.vocaloid_xsampa`, plus the Hangul
//! decomposition/romanisation of `libresvip.core.lyric_phoneme.korean`.
//!
//! Hanzi→Pinyin itself lives in the core crate
//! (`libresvip_core::core::lyric_phoneme::chinese`). `wanakana` is not available
//! in Rust, so [`to_romaji`] is a table-based approximation of its Hepburn
//! mapping; romaji input (the common case) passes through unchanged.

use libresvip_core::core::lyric_phoneme::japanese::LEGATO_CHARS;

pub fn is_legato_char(text: &str) -> bool {
    LEGATO_CHARS.contains(&text)
}

// ---------------------------------------------------------------------------
// Chinese: pinyin2xsampa (ported verbatim)
// ---------------------------------------------------------------------------

pub fn pinyin2xsampa(pinyin: &str) -> Option<&'static str> {
    PINYIN2XSAMPA
        .iter()
        .find(|(key, _)| *key == pinyin)
        .map(|(_, value)| *value)
}

static PINYIN2XSAMPA: &[(&str, &str)] = &[
    ("a", "a"),
    ("o", "o"),
    ("e", "7"),
    ("u", "u"),
    ("v", "y"),
    ("yu", "y"),
    ("yi", "i"),
    ("i", "i"),
    ("er", "@`"),
    ("ai", "aI"),
    ("ei", "ei"),
    ("ao", "AU"),
    ("ou", "@U"),
    ("an", "a_n"),
    ("en", "@_n"),
    ("ya", "ia"),
    ("ia", "ia"),
    ("ye", "iE_r"),
    ("ie", "iE_r"),
    ("yin", "i_n"),
    ("in", "i_n"),
    ("yong", "iUN"),
    ("iong", "iUN"),
    ("wu", "u"),
    ("wa", "ua"),
    ("ua", "ua"),
    ("wo", "uo"),
    ("uo", "uo"),
    ("wen", "u@_n"),
    ("uen", "u@_n"),
    ("un", "u@_n"),
    ("yue", "yE_r"),
    ("ve", "yE_r"),
    ("u:e", "yE_r"),
    ("yuan", "y{_n"),
    ("van", "y{_n"),
    ("u:an", "y{_n"),
    ("yun", "y_n"),
    ("vn", "y_n"),
    ("u:n", "y_n"),
    ("u:", "y"),
    ("ang", "AN"),
    ("yao", "iAU"),
    ("iao", "iAU"),
    ("you", "i@U"),
    ("iou", "i@U"),
    ("iu", "i@U"),
    ("wai", "uaI"),
    ("uai", "uaI"),
    ("wei", "uei"),
    ("uei", "uei"),
    ("ui", "uei"),
    ("weng", "u@N"),
    ("ueng", "u@N"),
    ("yan", "iE_n"),
    ("ian", "iE_n"),
    ("wan", "ua_n"),
    ("uan", "ua_n"),
    ("ying", "iN"),
    ("ing", "iN"),
    ("wang", "uAN"),
    ("uang", "uAN"),
    ("yang", "iAN"),
    ("iang", "iAN"),
    ("ba", "p a"),
    ("pa", "p_h a"),
    ("ma", "m a"),
    ("fa", "f a"),
    ("da", "t a"),
    ("ta", "t_h a"),
    ("na", "n a"),
    ("la", "l a"),
    ("ga", "k a"),
    ("ka", "k_h a"),
    ("ha", "x a"),
    ("zha", "ts` a"),
    ("cha", "ts`_h a"),
    ("sha", "s` a"),
    ("za", "ts a"),
    ("ca", "ts_h a"),
    ("sa", "s a"),
    ("bo", "p o"),
    ("po", "p_h o"),
    ("mo", "m o"),
    ("fo", "f o"),
    ("lo", "l o"),
    ("me", "m 7"),
    ("de", "t 7"),
    ("te", "t_h 7"),
    ("ne", "n 7"),
    ("le", "l 7"),
    ("ge", "k 7"),
    ("ke", "k_h 7"),
    ("he", "x 7"),
    ("zhe", "ts` 7"),
    ("che", "ts`_h 7"),
    ("she", "s` 7"),
    ("re", "z` 7"),
    ("ze", "ts 7"),
    ("ce", "ts_h 7"),
    ("se", "s 7"),
    ("zhi", "ts` i`"),
    ("chi", "ts`_h i`"),
    ("shi", "s` i`"),
    ("ri", "z` i`"),
    ("zi", "ts i\\"),
    ("ci", "ts_h i\\"),
    ("si", "s i\\"),
    ("bu", "p u"),
    ("pu", "p_h u"),
    ("mu", "m u"),
    ("fu", "f u"),
    ("du", "t u"),
    ("tu", "t_h u"),
    ("nu", "n u"),
    ("lu", "l u"),
    ("gu", "k u"),
    ("ku", "k_h u"),
    ("hu", "x u"),
    ("zhu", "ts` u"),
    ("chu", "ts`_h u"),
    ("shu", "s` u"),
    ("ru", "z` u"),
    ("zu", "ts u"),
    ("cu", "ts_h u"),
    ("su", "s u"),
    ("bei", "p ei"),
    ("pei", "p_h ei"),
    ("mei", "m ei"),
    ("fei", "f ei"),
    ("dei", "t ei"),
    ("nei", "n ei"),
    ("lei", "l ei"),
    ("gei", "k ei"),
    ("kei", "k_h ei"),
    ("hei", "x ei"),
    ("zhei", "ts` ei"),
    ("shei", "s` ei"),
    ("zei", "ts ei"),
    ("bao", "p AU"),
    ("pao", "p_h AU"),
    ("mao", "m AU"),
    ("dao", "t AU"),
    ("tao", "t_h AU"),
    ("nao", "n AU"),
    ("lao", "l AU"),
    ("gao", "k AU"),
    ("kao", "k_h AU"),
    ("hao", "x AU"),
    ("zhao", "ts` AU"),
    ("chao", "ts`_h AU"),
    ("shao", "s` AU"),
    ("rao", "z` AU"),
    ("zao", "ts AU"),
    ("cao", "ts_h AU"),
    ("sao", "s AU"),
    ("pou", "p_h @U"),
    ("mou", "m @U"),
    ("fou", "f @U"),
    ("dou", "t @U"),
    ("tou", "t_h @U"),
    ("nou", "n @U"),
    ("lou", "l @U"),
    ("gou", "k @U"),
    ("kou", "k_h @U"),
    ("hou", "x @U"),
    ("zhou", "ts` @U"),
    ("chou", "ts`_h @U"),
    ("shou", "s` @U"),
    ("rou", "z` @U"),
    ("zou", "ts @U"),
    ("cou", "ts_h @U"),
    ("sou", "s @U"),
    ("ban", "p a_n"),
    ("pan", "p_h a_n"),
    ("man", "m a_n"),
    ("fan", "f a_n"),
    ("dan", "t a_n"),
    ("tan", "t_h a_n"),
    ("nan", "n a_n"),
    ("lan", "l a_n"),
    ("gan", "k a_n"),
    ("kan", "k_h a_n"),
    ("han", "x a_n"),
    ("zhan", "ts` a_n"),
    ("chan", "ts`_h a_n"),
    ("shan", "s` a_n"),
    ("ran", "z` a_n"),
    ("zan", "ts a_n"),
    ("can", "ts_h a_n"),
    ("san", "s a_n"),
    ("ben", "p @_n"),
    ("pen", "p_h @_n"),
    ("men", "m @_n"),
    ("fen", "f @_n"),
    ("den", "t @_n"),
    ("nen", "n @_n"),
    ("gen", "k @_n"),
    ("ken", "k_h @_n"),
    ("hen", "x @_n"),
    ("zhen", "ts` @_n"),
    ("chen", "ts`_h @_n"),
    ("shen", "s` @_n"),
    ("ren", "z` @_n"),
    ("zen", "ts @_n"),
    ("cen", "ts_h @_n"),
    ("sen", "s @_n"),
    ("dong", "t UN"),
    ("tong", "t_h UN"),
    ("nong", "n UN"),
    ("long", "l UN"),
    ("gong", "k UN"),
    ("kong", "k_h UN"),
    ("hong", "x UN"),
    ("zhong", "ts` UN"),
    ("chong", "ts`_h UN"),
    ("rong", "z` UN"),
    ("zong", "ts UN"),
    ("cong", "ts_h UN"),
    ("song", "s UN"),
    ("bi", "p i"),
    ("pi", "p_h i"),
    ("mi", "m i"),
    ("di", "t i"),
    ("ti", "t_h i"),
    ("ni", "n i"),
    ("li", "l i"),
    ("ji", "ts\\ i"),
    ("qi", "ts\\_h i"),
    ("xi", "s\\ i"),
    ("gua", "k ua"),
    ("kua", "k_h ua"),
    ("hua", "x ua"),
    ("zhua", "ts` ua"),
    ("chua", "ts`_h ua"),
    ("shua", "s` ua"),
    ("dun", "t u@_n"),
    ("tun", "t_h u@_n"),
    ("lun", "l u@_n"),
    ("gun", "k u@_n"),
    ("kun", "k_h u@_n"),
    ("hun", "x u@_n"),
    ("zhun", "ts` u@_n"),
    ("chun", "ts`_h u@_n"),
    ("shun", "s` u@_n"),
    ("run", "z` u@_n"),
    ("zun", "ts u@_n"),
    ("cun", "ts_h u@_n"),
    ("sun", "s u@_n"),
    ("nu:", "n y"),
    ("lu:", "l y"),
    ("nv", "n y"),
    ("lv", "l y"),
    ("ju", "ts\\ y"),
    ("qu", "ts\\_h y"),
    ("xu", "s\\ y"),
    ("bai", "p aI"),
    ("pai", "p_h aI"),
    ("mai", "m aI"),
    ("dai", "t aI"),
    ("tai", "t_h aI"),
    ("nai", "n aI"),
    ("lai", "l aI"),
    ("gai", "k aI"),
    ("kai", "k_h aI"),
    ("hai", "x aI"),
    ("zhai", "ts` aI"),
    ("chai", "ts`_h aI"),
    ("shai", "s` aI"),
    ("zai", "ts aI"),
    ("cai", "ts_h aI"),
    ("sai", "s aI"),
    ("duo", "t uo"),
    ("tuo", "t_h uo"),
    ("nuo", "n uo"),
    ("luo", "l uo"),
    ("guo", "k uo"),
    ("kuo", "k_h uo"),
    ("huo", "x uo"),
    ("zhuo", "ts` uo"),
    ("chuo", "ts`_h uo"),
    ("shuo", "s` uo"),
    ("ruo", "z` uo"),
    ("zuo", "ts uo"),
    ("cuo", "ts_h uo"),
    ("suo", "s uo"),
    ("biao", "p iAU"),
    ("piao", "p_h iAU"),
    ("miao", "m iAU"),
    ("diao", "t iAU"),
    ("tiao", "t_h iAU"),
    ("niao", "n iAU"),
    ("liao", "l iAU"),
    ("jiao", "ts\\ iAU"),
    ("qiao", "ts\\_h iAU"),
    ("xiao", "s\\ iAU"),
    ("miu", "m i@U"),
    ("diu", "t i@U"),
    ("niu", "n i@U"),
    ("liu", "l i@U"),
    ("jiu", "ts\\ i@U"),
    ("qiu", "ts\\_h i@U"),
    ("xiu", "s\\ i@U"),
    ("guai", "k uaI"),
    ("kuai", "k_h uaI"),
    ("huai", "x uaI"),
    ("zhuai", "ts` uaI"),
    ("chuai", "ts`_h uaI"),
    ("shuai", "s` uaI"),
    ("dui", "t uei"),
    ("tui", "t_h uei"),
    ("gui", "k uei"),
    ("kui", "k_h uei"),
    ("hui", "x uei"),
    ("zhui", "ts` uei"),
    ("chui", "ts`_h uei"),
    ("shui", "s` uei"),
    ("rui", "z` uei"),
    ("zui", "ts uei"),
    ("cui", "ts_h uei"),
    ("sui", "s uei"),
    ("bang", "p AN"),
    ("pang", "p_h AN"),
    ("mang", "m AN"),
    ("fang", "f AN"),
    ("dang", "t AN"),
    ("tang", "t_h AN"),
    ("nang", "n AN"),
    ("lang", "l AN"),
    ("gang", "k AN"),
    ("kang", "k_h AN"),
    ("hang", "x AN"),
    ("zhang", "ts` AN"),
    ("chang", "ts`_h AN"),
    ("shang", "s` AN"),
    ("rang", "z` AN"),
    ("zang", "ts AN"),
    ("cang", "ts_h AN"),
    ("sang", "s AN"),
    ("beng", "p @N"),
    ("peng", "p_h @N"),
    ("meng", "m @N"),
    ("feng", "f @N"),
    ("deng", "t @N"),
    ("teng", "t_h @N"),
    ("neng", "n @N"),
    ("leng", "l @N"),
    ("geng", "k @N"),
    ("keng", "k_h @N"),
    ("heng", "x @N"),
    ("zheng", "ts` @N"),
    ("cheng", "ts`_h @N"),
    ("sheng", "s` @N"),
    ("reng", "z` @N"),
    ("zeng", "ts @N"),
    ("ceng", "ts_h @N"),
    ("seng", "s @N"),
    ("dia", "t ia"),
    ("lia", "l ia"),
    ("jia", "ts\\ ia"),
    ("qia", "ts\\_h ia"),
    ("xia", "s\\ ia"),
    ("bie", "p iE_r"),
    ("pie", "p_h iE_r"),
    ("mie", "m iE_r"),
    ("die", "t iE_r"),
    ("tie", "t_h iE_r"),
    ("nie", "n iE_r"),
    ("lie", "l iE_r"),
    ("jie", "ts\\ iE_r"),
    ("qie", "ts\\_h iE_r"),
    ("xie", "s\\ iE_r"),
    ("bin", "p i_n"),
    ("pin", "p_h i_n"),
    ("min", "m i_n"),
    ("nin", "n i_n"),
    ("lin", "l i_n"),
    ("jin", "ts\\ i_n"),
    ("qin", "ts\\_h i_n"),
    ("xin", "s\\ i_n"),
    ("juan", "ts\\ y{_n"),
    ("quan", "ts\\_h y{_n"),
    ("xuan", "s\\ y{_n"),
    ("nve", "n yE_r"),
    ("lve", "l yE_r"),
    ("jue", "ts\\ yE_r"),
    ("que", "ts\\_h yE_r"),
    ("xue", "s\\ yE_r"),
    ("nu:e", "n yE_r"),
    ("lu:e", "l yE_r"),
    ("duan", "t ua_n"),
    ("tuan", "t_h ua_n"),
    ("nuan", "n ua_n"),
    ("luan", "l ua_n"),
    ("guan", "k ua_n"),
    ("kuan", "k_h ua_n"),
    ("huan", "x ua_n"),
    ("zhuan", "ts` ua_n"),
    ("chuan", "ts`_h ua_n"),
    ("shuan", "s` ua_n"),
    ("ruan", "z` ua_n"),
    ("zuan", "ts ua_n"),
    ("cuan", "ts_h ua_n"),
    ("suan", "s ua_n"),
    ("jun", "ts\\ y_n"),
    ("qun", "ts\\_h y_n"),
    ("xun", "s\\ y_n"),
    ("bing", "p iN"),
    ("ping", "p_h iN"),
    ("ming", "m iN"),
    ("ding", "t iN"),
    ("ting", "t_h iN"),
    ("ning", "n iN"),
    ("ling", "l iN"),
    ("jing", "ts\\ iN"),
    ("qing", "ts\\_h iN"),
    ("xing", "s\\ iN"),
    ("jiong", "ts\\ iUN"),
    ("qiong", "ts\\_h iUN"),
    ("xiong", "s\\ iUN"),
    ("guang", "k uAN"),
    ("kuang", "k_h uAN"),
    ("huang", "x uAN"),
    ("zhuang", "ts` uAN"),
    ("chuang", "ts`_h uAN"),
    ("shuang", "s` uAN"),
    ("bian", "p iE_n"),
    ("pian", "p_h iE_n"),
    ("mian", "m iE_n"),
    ("dian", "t iE_n"),
    ("tian", "t_h iE_n"),
    ("nian", "n iE_n"),
    ("lian", "l iE_n"),
    ("jian", "ts\\ iE_n"),
    ("qian", "ts\\_h iE_n"),
    ("xian", "s\\ iE_n"),
    ("niang", "n iAN"),
    ("liang", "l iAN"),
    ("jiang", "ts\\ iAN"),
    ("qiang", "ts\\_h iAN"),
    ("xiang", "s\\ iAN"),
];

// ---------------------------------------------------------------------------
// Japanese: romaji2xsampa (ported verbatim)
// ---------------------------------------------------------------------------

pub fn romaji2xsampa(romaji: &str) -> Option<&'static str> {
    ROMAJI2XSAMPA
        .iter()
        .find(|(key, _)| *key == romaji)
        .map(|(_, value)| *value)
}

static ROMAJI2XSAMPA: &[(&str, &str)] = &[
    ("n", "n"),
    ("m", "m"),
    ("a", "a"),
    ("i", "i"),
    ("u", "M"),
    ("e", "e"),
    ("o", "o"),
    ("ka", "k a"),
    ("ki", "k' i"),
    ("ku", "k M"),
    ("ke", "k e"),
    ("ko", "k o"),
    ("sa", "s a"),
    ("shi", "S i"),
    ("su", "s M"),
    ("se", "s e"),
    ("so", "s o"),
    ("ta", "t a"),
    ("chi", "tS i"),
    ("tsu", "ts M"),
    ("te", "t e"),
    ("to", "t o"),
    ("na", "n a"),
    ("ni", "J i"),
    ("nu", "n M"),
    ("ne", "n e"),
    ("no", "n o"),
    ("ha", "h a"),
    ("hi", "C i"),
    ("fu", "p\\ M"),
    ("he", "h e"),
    ("ho", "h o"),
    ("ma", "m a"),
    ("mi", "m' i"),
    ("mu", "m M"),
    ("me", "m e"),
    ("mo", "m o"),
    ("ra", "4 a"),
    ("ri", "4' i"),
    ("ru", "4 M"),
    ("re", "4 e"),
    ("ro", "4 o"),
    ("ga", "g a"),
    ("gi", "g' i"),
    ("gu", "g M"),
    ("ge", "g e"),
    ("go", "g o"),
    ("za", "dz a"),
    ("ji", "dZ i"),
    ("zu", "dz M"),
    ("ze", "dz e"),
    ("zo", "dz o"),
    ("da", "d a"),
    ("de", "d e"),
    ("do", "d o"),
    ("ba", "b a"),
    ("bi", "b' i"),
    ("bu", "b M"),
    ("be", "b e"),
    ("bo", "b o"),
    ("pa", "p a"),
    ("pi", "p' i"),
    ("pu", "p M"),
    ("pe", "p e"),
    ("po", "p o"),
    ("ya", "j a"),
    ("yu", "j M"),
    ("yo", "j o"),
    ("wa", "w a"),
    ("wi", "w i"),
    ("we", "w e"),
    ("wo", "w o"),
    ("ltu", "Sil"),
    ("ltsu", "Sil"),
    ("xtu", "Sil"),
    ("xtsu", "Sil"),
    ("fa", "p\\ a"),
    ("tsa", "ts a"),
    ("si", "s i"),
    ("zi", "dz i"),
    ("tsi", "ts i"),
    ("ti", "t' i"),
    ("di", "d' i"),
    ("fi", "p\\' i"),
    ("tu", "t M"),
    ("du", "d M"),
    ("ye", "j e"),
    ("kye", "k' e"),
    ("she", "S e"),
    ("che", "tS e"),
    ("tse", "ts e"),
    ("tye", "t' e"),
    ("nye", "J e"),
    ("hye", "C e"),
    ("mye", "m' e"),
    ("rye", "4' e"),
    ("gye", "g' e"),
    ("je", "dZ e"),
    ("dye", "d' e"),
    ("bye", "b' e"),
    ("pye", "p' e"),
    ("fe", "p\\ e"),
    ("tso", "ts o"),
    ("fo", "p\\ o"),
    ("kya", "k' a"),
    ("sha", "S a"),
    ("cha", "tS a"),
    ("tya", "t' a"),
    ("nya", "J a"),
    ("hya", "C a"),
    ("mya", "m' a"),
    ("rya", "4' a"),
    ("gya", "g' a"),
    ("ja", "dZ a"),
    ("dya", "d' a"),
    ("bya", "b' a"),
    ("pya", "p' a"),
    ("fya", "p\\' a"),
    ("kyu", "k' M"),
    ("shu", "S M"),
    ("chu", "tS M"),
    ("tyu", "t' M"),
    ("nyu", "J M"),
    ("hyu", "C M"),
    ("myu", "m' M"),
    ("ryu", "4' M"),
    ("gyu", "g' M"),
    ("ju", "dZ M"),
    ("dyu", "d' M"),
    ("byu", "b' M"),
    ("pyu", "p' M"),
    ("fyu", "p\\' M"),
    ("kyo", "k' o"),
    ("sho", "S o"),
    ("cho", "tS o"),
    ("tyo", "t' o"),
    ("nyo", "J o"),
    ("hyo", "C o"),
    ("myo", "m' o"),
    ("ryo", "4' o"),
    ("gyo", "g' o"),
    ("jo", "dZ o"),
    ("dyo", "d' o"),
    ("byo", "b' o"),
    ("pyo", "p' o"),
];

// ---------------------------------------------------------------------------
// Japanese: to_romaji
// ---------------------------------------------------------------------------

/// Hiragana digraphs that `wanakana` combines (consonant + small kana).
/// Extracted from `wanakana.to_romaji`; katakana is folded to hiragana first.
static KANA_DIGRAPHS: &[(&str, &str)] = &[
    ("きぃ", "kyi"),
    ("きぇ", "kye"),
    ("きゃ", "kya"),
    ("きゅ", "kyu"),
    ("きょ", "kyo"),
    ("くぃ", "kyi"),
    ("くぇ", "kye"),
    ("くゃ", "kya"),
    ("くゅ", "kyu"),
    ("くょ", "kyo"),
    ("しぃ", "shyi"),
    ("しぇ", "she"),
    ("しゃ", "sha"),
    ("しゅ", "shu"),
    ("しょ", "sho"),
    ("ちぃ", "chyi"),
    ("ちぇ", "che"),
    ("ちゃ", "cha"),
    ("ちゅ", "chu"),
    ("ちょ", "cho"),
    ("にぃ", "nyi"),
    ("にぇ", "nye"),
    ("にゃ", "nya"),
    ("にゅ", "nyu"),
    ("にょ", "nyo"),
    ("ひぃ", "hyi"),
    ("ひぇ", "hye"),
    ("ひゃ", "hya"),
    ("ひゅ", "hyu"),
    ("ひょ", "hyo"),
    ("ふぃ", "fyi"),
    ("ふぇ", "fye"),
    ("ふゃ", "fya"),
    ("ふゅ", "fyu"),
    ("ふょ", "fyo"),
    ("みぃ", "myi"),
    ("みぇ", "mye"),
    ("みゃ", "mya"),
    ("みゅ", "myu"),
    ("みょ", "myo"),
    ("りぃ", "ryi"),
    ("りぇ", "rye"),
    ("りゃ", "rya"),
    ("りゅ", "ryu"),
    ("りょ", "ryo"),
    ("ぎぃ", "gyi"),
    ("ぎぇ", "gye"),
    ("ぎゃ", "gya"),
    ("ぎゅ", "gyu"),
    ("ぎょ", "gyo"),
    ("じぃ", "jyi"),
    ("じぇ", "je"),
    ("じゃ", "ja"),
    ("じゅ", "ju"),
    ("じょ", "jo"),
    ("ぢぃ", "djyi"),
    ("ぢぇ", "dje"),
    ("ぢゃ", "dja"),
    ("ぢゅ", "dju"),
    ("ぢょ", "djo"),
    ("びぃ", "byi"),
    ("びぇ", "bye"),
    ("びゃ", "bya"),
    ("びゅ", "byu"),
    ("びょ", "byo"),
    ("ぴぃ", "pyi"),
    ("ぴぇ", "pye"),
    ("ぴゃ", "pya"),
    ("ぴゅ", "pyu"),
    ("ぴょ", "pyo"),
    ("ゔぃ", "vyi"),
    ("ゔぇ", "vye"),
    ("ゔゃ", "vya"),
    ("ゔゅ", "vyu"),
    ("ゔょ", "vyo"),
];

/// Single hiragana → romaji, matching `wanakana.to_romaji`.
static KANA_ROMAJI: &[(&str, &str)] = &[
    ("あ", "a"),
    ("い", "i"),
    ("う", "u"),
    ("え", "e"),
    ("お", "o"),
    ("か", "ka"),
    ("き", "ki"),
    ("く", "ku"),
    ("け", "ke"),
    ("こ", "ko"),
    ("が", "ga"),
    ("ぎ", "gi"),
    ("ぐ", "gu"),
    ("げ", "ge"),
    ("ご", "go"),
    ("さ", "sa"),
    ("し", "shi"),
    ("す", "su"),
    ("せ", "se"),
    ("そ", "so"),
    ("ざ", "za"),
    ("じ", "ji"),
    ("ず", "zu"),
    ("ぜ", "ze"),
    ("ぞ", "zo"),
    ("た", "ta"),
    ("ち", "chi"),
    ("つ", "tsu"),
    ("て", "te"),
    ("と", "to"),
    ("だ", "da"),
    ("ぢ", "dji"),
    ("づ", "dzu"),
    ("で", "de"),
    ("ど", "do"),
    ("な", "na"),
    ("に", "ni"),
    ("ぬ", "nu"),
    ("ね", "ne"),
    ("の", "no"),
    ("は", "ha"),
    ("ひ", "hi"),
    ("ふ", "fu"),
    ("へ", "he"),
    ("ほ", "ho"),
    ("ば", "ba"),
    ("び", "bi"),
    ("ぶ", "bu"),
    ("べ", "be"),
    ("ぼ", "bo"),
    ("ぱ", "pa"),
    ("ぴ", "pi"),
    ("ぷ", "pu"),
    ("ぺ", "pe"),
    ("ぽ", "po"),
    ("ま", "ma"),
    ("み", "mi"),
    ("む", "mu"),
    ("め", "me"),
    ("も", "mo"),
    ("や", "ya"),
    ("ゆ", "yu"),
    ("よ", "yo"),
    ("ら", "ra"),
    ("り", "ri"),
    ("る", "ru"),
    ("れ", "re"),
    ("ろ", "ro"),
    ("わ", "wa"),
    ("ゐ", "wi"),
    ("ゑ", "we"),
    ("を", "wo"),
    ("ん", "n"),
    ("ぁ", "a"),
    ("ぃ", "i"),
    ("ぅ", "u"),
    ("ぇ", "e"),
    ("ぉ", "o"),
    ("ゃ", "ya"),
    ("ゅ", "yu"),
    ("ょ", "yo"),
    ("ゔ", "vu"),
];

fn kata_to_hira(character: char) -> char {
    let code = character as u32;
    // ヵ (U+30F5) and ヶ (U+30F6) have no hiragana counterpart in wanakana.
    if (0x30A1..=0x30F4).contains(&code) {
        char::from_u32(code - 0x60).unwrap_or(character)
    } else {
        character
    }
}

/// One kana unit (digraph or single character) at `index`.
fn romaji_unit(folded: &[char], index: usize) -> Option<(&'static str, usize)> {
    let current = *folded.get(index)?;
    if let Some(next) = folded.get(index + 1) {
        let pair: String = [current, *next].iter().collect();
        if let Some((_, romaji)) = KANA_DIGRAPHS.iter().find(|(key, _)| *key == pair) {
            return Some((*romaji, 2));
        }
    }
    let single = current.to_string();
    KANA_ROMAJI
        .iter()
        .find(|(key, _)| *key == single)
        .map(|(_, romaji)| (*romaji, 1))
}

/// `libresvip.core.lyric_phoneme.japanese.to_romaji` (a table-based port of
/// `wanakana.to_romaji` with `custom_romaji_mapping={"っ": "cl"}`).
pub fn to_romaji(word: &str) -> String {
    if word.is_empty() {
        return String::new();
    }
    let folded: Vec<char> = word.chars().map(kata_to_hira).collect();
    let mut result = String::new();
    let mut index = 0usize;
    while index < folded.len() {
        let character = folded[index];
        if character == 'っ' {
            match folded.get(index + 1).copied() {
                // Standalone / before another small tsu / non-kana: "cl".
                None => result.push_str("cl"),
                Some(next) if next == 'っ' => result.push_str("cl"),
                // Before a long vowel mark the small tsu disappears.
                Some(next) if next == 'ー' => {}
                Some(_) => match romaji_unit(&folded, index + 1) {
                    Some((romaji, _)) => {
                        let first = romaji.chars().next().unwrap_or(' ');
                        // wanakana doubles consonant onsets but not ん/や行.
                        if first.is_ascii_alphabetic() && first != 'y' && first != 'n' {
                            if romaji.starts_with("ch") {
                                result.push('t');
                            } else {
                                result.push(first);
                            }
                        }
                    }
                    None => result.push_str("cl"),
                },
            }
            index += 1;
            continue;
        }
        if character == 'ー' {
            result.push('-');
            index += 1;
            continue;
        }
        if let Some((romaji, length)) = romaji_unit(&folded, index) {
            result.push_str(romaji);
            index += length;
        } else {
            result.push(character);
            index += 1;
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Korean: hangul2xsampa (port of `ko_pron`'s jamo romanisation)
// ---------------------------------------------------------------------------


const KOREAN_INITIALS: [(&str, &str); 19] = [
    ("ᄀ", "g"),
    ("ᄁ", "kk"),
    ("ᄂ", "n"),
    ("ᄃ", "d"),
    ("ᄄ", "tt"),
    ("ᄅ", "r"),
    ("ᄆ", "m"),
    ("ᄇ", "b"),
    ("ᄈ", "pp"),
    ("ᄉ", "s"),
    ("ᄊ", "ss"),
    ("ᄋ", ""),
    ("ᄌ", "j"),
    ("ᄍ", "jj"),
    ("ᄎ", "ch"),
    ("ᄏ", "k"),
    ("ᄐ", "t"),
    ("ᄑ", "p"),
    ("ᄒ", "h"),
];

const KOREAN_VOWELS: [(&str, &str); 21] = [
    ("ᅡ", "a"),
    ("ᅢ", "ae"),
    ("ᅣ", "ya"),
    ("ᅤ", "yae"),
    ("ᅥ", "eo"),
    ("ᅦ", "e"),
    ("ᅧ", "yeo"),
    ("ᅨ", "ye"),
    ("ᅩ", "o"),
    ("ᅪ", "wa"),
    ("ᅫ", "wae"),
    ("ᅬ", "oe"),
    ("ᅭ", "yo"),
    ("ᅮ", "u"),
    ("ᅯ", "wo"),
    ("ᅰ", "we"),
    ("ᅱ", "wi"),
    ("ᅲ", "yu"),
    ("ᅳ", "eu"),
    ("ᅴ", "ui"),
    ("ᅵ", "i"),
];

/// Jongseong index → romanised final (index 0 is "no final").
const KOREAN_FINALS: [&str; 28] = [
    "", "k", "k", "k", "n", "n", "n", "t", "l", "k", "m", "l", "l", "l", "p", "l", "m", "p", "p",
    "t", "t", "ng", "t", "t", "k", "t", "p", "",
];

const ROMAJI2KOREAN_XSAMPA: [(&str, &str); 58] = [
    ("", ""),
    ("a", "a"),
    ("ya", "ja"),
    ("eo", "7"),
    ("yeo", "j7"),
    ("ae", "e"),
    ("e", "e"),
    ("yae", "je"),
    ("ye", "je"),
    ("o", "o"),
    ("wa", "oa"),
    ("yo", "jo"),
    ("u", "u"),
    ("weo", "u7"),
    ("wae", "ue"),
    ("we", "ue"),
    ("oe", "ue"),
    ("wi", "ui"),
    ("eu", "M"),
    ("eui", "Mi"),
    ("i", "i"),
    ("g", "g"),
    ("gg", "g'"),
    ("n", "n"),
    ("d", "d"),
    ("dd", "d'"),
    ("r", "r"),
    ("l", "l"),
    ("m", "m"),
    ("b", "b"),
    ("bb", "b'"),
    ("s", "s"),
    ("ss", "s'"),
    ("j", "c"),
    ("jj", "c'"),
    ("ch", "ch"),
    ("k", "k"),
    ("t", "t"),
    ("p", "p"),
    ("h", "h"),
    ("ng", "Np"),
    ("gs", "gp"),
    ("nch", "np"),
    ("nh", "np"),
    ("kk", "g'"),
    ("tt", "d'"),
    ("pp", "b'"),
    ("wo", "a"),
    ("yu", "ju"),
    ("ui", "i"),
    ("lg", "gp"),
    ("lm", "mp"),
    ("lb", "rp"),
    ("ls", "rp"),
    ("lt", "rp"),
    ("lp", "bp"),
    ("lh", "rp"),
    ("ps", "bp"),
];

const ROMAJI2KOREAN_XSAMPA_FINAL: [(&str, &str); 6] = [
    ("k", "gp"),
    ("n", "np"),
    ("t", "dp"),
    ("l", "rp"),
    ("m", "mp"),
    ("p", "bp"),
];

fn korean_initial(character: char) -> Option<&'static str> {
    let code = character as u32;
    if (0x1100..=0x1112).contains(&code) {
        Some(KOREAN_INITIALS[(code - 0x1100) as usize].1)
    } else {
        None
    }
}

fn korean_vowel(character: char) -> Option<&'static str> {
    let code = character as u32;
    if (0x1161..=0x1175).contains(&code) {
        Some(KOREAN_VOWELS[(code - 0x1161) as usize].1)
    } else {
        None
    }
}

fn korean_final(character: char) -> Option<&'static str> {
    let code = character as u32;
    if (0x11A8..=0x11C2).contains(&code) {
        Some(KOREAN_FINALS[(code - 0x11A7) as usize])
    } else {
        None
    }
}

/// `ko_pron.decompose_syllable(syllable)[0]` → (initial, vowel, final) jamo.
fn decompose_syllable(syllable: char) -> (String, String, String) {
    let code = syllable as u32;
    let is_jamo = |code: u32, start: u32, end: u32| (start..=end).contains(&code);
    if is_jamo(code, 0xAC00, 0xD7A3) {
        let relative = code - 0xAC00;
        let jongseong = relative % 28;
        let jungseong = (relative % 588) / 28;
        let choseong = relative / 588;
        let initial = char::from_u32(0x1100 + choseong)
            .map(|value| value.to_string())
            .unwrap_or_default();
        let vowel = char::from_u32(0x1161 + jungseong)
            .map(|value| value.to_string())
            .unwrap_or_default();
        let final_jamo = if jongseong != 0 {
            char::from_u32(0x11A7 + jongseong)
                .map(|value| value.to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        return (initial, vowel, final_jamo);
    }
    if is_jamo(code, 0x1100, 0x1112) {
        return (syllable.to_string(), "Ø".to_owned(), "Ø".to_owned());
    }
    if is_jamo(code, 0x1161, 0x1175) {
        return ("Ø".to_owned(), syllable.to_string(), "Ø".to_owned());
    }
    if is_jamo(code, 0x11A8, 0x11C2) {
        return ("Ø".to_owned(), "Ø".to_owned(), syllable.to_string());
    }
    if is_jamo(code, 0x3131, 0x318E) {
        return ("Ø".to_owned(), "Ø".to_owned(), syllable.to_string());
    }
    ("Ø".to_owned(), " ".to_owned(), "X".to_owned())
}

fn romanise_jamo(jamo: &str) -> String {
    if let Some(character) = jamo.chars().next() {
        if let Some(romanised) = korean_initial(character) {
            return romanised.to_owned();
        }
        if let Some(romanised) = korean_vowel(character) {
            return romanised.to_owned();
        }
        if let Some(romanised) = korean_final(character) {
            return romanised.to_owned();
        }
    }
    // Unmatched characters (`Ø`, ` `, `X`, compatibility jamo) pass through.
    jamo.to_owned()
}

fn xsampa_lookup(table: &[(&str, &str)], key: &str) -> String {
    table
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| (*value).to_owned())
        .unwrap_or_default()
}

/// `hangul2xsampa` from `libresvip.core.lyric_phoneme.korean.vocaloid_xsampa`.
pub fn hangul2xsampa(lyric: &str) -> String {
    let mut phonemes: Vec<String> = Vec::new();
    for syllable in lyric.chars() {
        let (initial, vowel, final_jamo) = decompose_syllable(syllable);
        let initial_romaji = romanise_jamo(&initial);
        let vowel_romaji = romanise_jamo(&vowel);
        let final_romaji = romanise_jamo(&final_jamo);
        let mut initial_xsampa = xsampa_lookup(&ROMAJI2KOREAN_XSAMPA, &initial_romaji);
        let mut vowel_xsampa = xsampa_lookup(&ROMAJI2KOREAN_XSAMPA, &vowel_romaji);
        let mut final_xsampa = xsampa_lookup(&ROMAJI2KOREAN_XSAMPA, &final_romaji);
        let starts_iotised = |value: &str| {
            value.starts_with('i') || value.starts_with('y') || value.starts_with('j')
        };
        if (initial_xsampa == "s" || initial_xsampa == "sh") && starts_iotised(&vowel_xsampa) {
            initial_xsampa = if initial_xsampa == "s" {
                "sh".to_owned()
            } else {
                "sh'".to_owned()
            };
        } else if (vowel_xsampa == "s" || vowel_xsampa == "sh") && starts_iotised(&final_xsampa) {
            vowel_xsampa = if vowel_xsampa == "s" {
                "sh".to_owned()
            } else {
                "sh'".to_owned()
            };
        }
        if let Some((_, value)) = ROMAJI2KOREAN_XSAMPA_FINAL
            .iter()
            .find(|(name, _)| *name == final_xsampa)
        {
            final_xsampa = (*value).to_owned();
        }
        phonemes.push(format!("{initial_xsampa} {vowel_xsampa} {final_xsampa}").trim().to_owned());
    }
    if phonemes.is_empty() {
        "r a".to_owned()
    } else {
        phonemes.join(" ")
    }
}

