"""Generate differential test vectors for the lyric/phoneme helpers from Python.

Writes `testdata/phoneme_cases.json`, consumed by
`crates/libresvip-core/tests/phoneme_cases.rs`.

The corpus covers:

* every character of pypinyin's `pinyin_dict` (lazy_pinyin + get_pinyin_series),
* every entry of pypinyin's `phrases_dict` (segmentation / polyphonic words),
* mixed Chinese / English / hyphen / whitespace lyrics and the non-default
  `get_pinyin_series` flags,
* kana (single, digraphs, sokuon, long vowels, symbols), katakana, romaji and
  mixed strings for `to_romaji`,
* every Hangul syllable, every final→initial junction, curated words and mixed
  text for `ko_pron` romanisation and `hangul2xsampa`.

A `null` expectation means the Python implementation raised; the Rust side must
return `Err`/`None` for those.

Usage:
    E:\\miniconda\\envs\\study12\\python.exe scripts/check_phoneme.py
"""

from __future__ import annotations

import json
import pathlib
import random
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
PY_ROOT = ROOT.parent / "LibreSVIP"
if str(PY_ROOT) not in sys.path:
    sys.path.insert(0, str(PY_ROOT))

from ko_pron.ko_pron import romanise  # noqa: E402
from pypinyin.constants import PINYIN_DICT, PHRASES_DICT  # noqa: E402
from pypinyin import lazy_pinyin as pypinyin_lazy_pinyin  # noqa: E402

from libresvip.core.lyric_phoneme.chinese import get_pinyin_series  # noqa: E402
from libresvip.core.lyric_phoneme.japanese import to_romaji  # noqa: E402
from libresvip.core.lyric_phoneme.korean.vocaloid_xsampa import hangul2xsampa  # noqa: E402

TESTDATA = ROOT / "testdata"

HIRAGANA = [chr(code) for code in range(0x3041, 0x3097)]
KATAKANA = [chr(code) for code in range(0x30A1, 0x30FD)]
SMALL_KANA = "ゃゅょぁぃぅぇぉっ"
KANA_SYMBOLS = "、。：・！？〜「」『』［］（）｛｝　ー"
ROMAJI_ALPHABET = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 -'./!?"

# A fixed list so the corpus is reproducible.
KOREAN_WORDS = [
    "사랑",
    "사랑해",
    "안녕",
    "안녕하세요",
    "한국",
    "대한민국",
    "서울",
    "고마워",
    "미안해",
    "괜찮아",
    "행복",
    "눈물",
    "하늘",
    "바다",
    "별빛",
    "너와나",
    "우리",
    "영원히",
    "기억",
    "약속",
    "그리움",
    "첫눈",
    "겨울",
    "봄날",
    "여름밤",
    "가을",
    "편지",
    "노래",
    "마음",
    "심장",
    "숨소리",
    "이별",
    "만남",
    "추억",
    "시간",
    "오늘",
    "내일",
    "어제",
    "사람",
    "세상",
    "꿈",
    "희망",
    "빛나",
    "반짝",
    "따뜻",
    "학교",
    "친구",
    "가족",
    "엄마",
    "아빠",
    "동생",
    "형님",
    "누나",
    "값",
    "삶",
    "닭",
    "맑다",
    "흙",
    "꽃",
    "빛",
    "있어",
    "없어",
    "앉아",
    "많아",
    "읽어",
    "젊은",
    "앓아",
    "넓어",
    "짧아",
    "밟아",
    "핥아",
    "몫",
    "섞여",
    "한강",
    "감사합니다",
    "사랑합니다",
    "보고싶어",
    "그대여",
    "오빠",
    "언니",
    "동화",
    "미소",
    "약해",
    "굳어",
    "좋아",
    "싫어",
    "알아",
    "몰라",
    "웃어",
    "울어",
    "잡아",
    "놓아",
    "사랑 you",
    "안녕!",
    "K-POP",
    "'사랑'",
    "우리 사랑",
    "",
    " ",
    "ㄱ",
    "ㄱㄴ",
    "ㅏ",
    "ᆨ",
    "ᄀ",
    "가 나",
    "가!나",
    "가\n나",
    '가"나',
    "가#나",
    "가$나",
    "가%나",
    "혼자",
    "둘이서",
    "밤하늘",
    # `ko_pron` feeds the text through `re.sub` as a pattern, so metacharacters
    # change which part of the lyric is replaced.
    "(웃음)",
    "가(나)",
    "[사랑]",
    "가|나",
    "가*나",
    "가+나",
    "가?나",
    "(안녕) 사랑",
    "가.나",
    "가^나",
    "hello (가)",
    "(가) (나)",
    "((가))",
    "가.나.다",
    "가(나",
    "가)나",
    "가[나",
]


def capture(func, *args, **kwargs):
    """Call `func`, returning `None` when the Python implementation raises."""
    try:
        return func(*args, **kwargs)
    except Exception:  # noqa: BLE001 - the corpus records crashes as `null`
        return None


def pinyin_characters() -> list[list[object]]:
    cases = []
    for codepoint in sorted(PINYIN_DICT):
        character = chr(codepoint)
        lazy = pypinyin_lazy_pinyin(character, errors=lambda _: " ")
        series = get_pinyin_series([character])
        cases.append([character, lazy, series])
    return cases


def lazy_pinyin_cases() -> list[list[str]]:
    """Every phrase from pypinyin's phrase dictionary, plus mixed text."""
    phrases = [
        phrase for phrase in sorted(PHRASES_DICT) if len(phrase) > 1
    ]
    texts = list(phrases)
    texts += [
        "",
        " ",
        "你好",
        "你好世界",
        "中国",
        "测试abc测试",
        "hello world",
        "a b",
        "a-b",
        "你-a",
        "A你b好",
        "あいう",
        "。",
        "，",
        "你 好",
        "你好，世界",
        "ABC",
        "中",
        "謝謝",
        "〇",
        "I love you 我爱你",
        "La La La 啦啦啦",
        "hello-world",
        "你-好",
        "混ぜる",
        "한국어",
        "中\u30fc文",
    ]
    return [
        [text, pypinyin_lazy_pinyin(text, errors=lambda _: " ")]
        for text in texts
    ]


def series_cases() -> list[list[object]]:
    tokens = [
        "你好",
        "abc",
        "AB",
        "-",
        " ",
        "，",
        "中",
        "好",
        "x1",
        "あ",
        "你 好",
        "a-b",
        "中国",
        "hello world",
        "",
        "\U0002a700\U0002a701",
        "中\U0002b81e\U0002b81f好",
    ]
    parts_list = [[token] for token in tokens]
    parts_list += [[left, right] for left in tokens for right in tokens]
    parts_list += [
        [],
        ["你好", "abc", "世界"],
        ["abc", "你好", "def"],
        ["-", "你", "-", "好", "-"],
        ["a", "你", "b", "好"],
        ["\U0002a700", "中"],
        ["中", "\U0002a700"],
        ["\U0002a700\U0002a701", "中"],
        ["中", "\U0002a700\U0002a701"],
    ]

    cases: list[list[object]] = []
    for parts in parts_list:
        expected = capture(get_pinyin_series, parts)
        cases.append([parts, None, expected])

    flag_sets = [
        {"ignore_hyphens": False},
        {"reverse_letters": False},
        {"filter_non_chinese": False},
        {"ignore_hyphens": False, "reverse_letters": False},
        {"ignore_hyphens": False, "filter_non_chinese": False},
        {"reverse_letters": False, "filter_non_chinese": False},
        {"ignore_hyphens": False, "reverse_letters": False, "filter_non_chinese": False},
    ]
    flag_tokens = [
        "你-好",
        "a-b",
        "a：b",
        "你，好",
        "Hello",
        "abc你def",
        "A你b好",
        "-",
        "你 好",
        "a1-b2",
    ]
    for flags in flag_sets:
        for token in flag_tokens:
            expected = capture(get_pinyin_series, [token], **flags)
            cases.append([[token], flags, expected])
    return cases


def random_lyrics_cases() -> list[list[object]]:
    """Random mixed Chinese / Latin / digit / punctuation / kana lyrics."""
    rng = random.Random(987654321)
    hanzi = [
        chr(codepoint)
        for codepoint in rng.sample(sorted(PINYIN_DICT), 400)
    ]
    pool = (
        hanzi
        + list("abcdefgxyzABXYZ0123456789 -_.'!?")
        + list("，。！？、；：（）")
        + list("あいうえおかきくけこアイウエオ")
    )
    cases = []
    for _ in range(1200):
        length = rng.randint(1, 14)
        text = "".join(rng.choice(pool) for _ in range(length))
        parts: list[str] = []
        if rng.random() < 0.5:
            parts = [text]
        else:
            remaining = text
            while remaining and len(parts) < 3:
                cut = rng.randint(1, len(remaining))
                parts.append(remaining[:cut])
                remaining = remaining[cut:]
            if remaining:
                parts.append(remaining)
        cases.append(
            [
                text,
                pypinyin_lazy_pinyin(text, errors=lambda _: " "),
                parts,
                capture(get_pinyin_series, parts),
            ]
        )
    return cases


def to_romaji_cases() -> list[list[object]]:
    texts: list[str] = []
    texts += HIRAGANA
    texts += KATAKANA
    texts += list(KANA_SYMBOLS)
    texts += [kana + small for kana in HIRAGANA for small in SMALL_KANA]
    texts += [kana + small for kana in KATAKANA for small in SMALL_KANA]
    texts += [kana + "ー" for kana in HIRAGANA[:60]]
    texts += [kana + "ー" for kana in KATAKANA[:60]]
    texts += [
        "",
        "こんにちは",
        "コンニチハ",
        "こんばんは",
        "ありがとう",
        "きって",
        "まっちゃ",
        "がっこう",
        "しんぶん",
        "コーヒー",
        "スーパー",
        "ラーメン",
        "らーめん",
        "テスト・ケース",
        "ヴァイオリン",
        "ゔぁ",
        "abc",
        "ABC",
        "hello world",
        "a-i-u",
        "ｱｲｳｴｵ",
        "こんにちは、世界",
        "あ、い。う",
        "ＡＢＣ",
        "あー",
        "ンー",
        "っー",
        "ー",
        "・",
        "ゕゖ",
        "ヵヶ",
        "ヷヸヹヺ",
        "ゐゑを",
        "ヰヱヲ",
        "《あ》",
        "あ\u3000い",
    ]

    rng = random.Random(20240523)
    pool = HIRAGANA + KATAKANA + list(SMALL_KANA) + list(KANA_SYMBOLS) + list(ROMAJI_ALPHABET)
    for _ in range(600):
        length = rng.randint(1, 8)
        texts.append("".join(rng.choice(pool) for _ in range(length)))

    return [[text, capture(to_romaji, text)] for text in texts]


def korean_cases() -> list[list[object]]:
    texts: list[str] = [chr(code) for code in range(0xAC00, 0xD7A4)]
    # Every final → initial junction, plus every vowel in a leading position.
    for jongseong in range(0, 28):
        for choseong in range(0, 19):
            first = chr(0xAC00 + choseong * 588 + 0 * 28 + jongseong)
            second = chr(0xAC00 + choseong * 588)
            texts.append(first + second)
    for jungseong in range(0, 21):
        texts.append(chr(0xAC00 + jungseong * 28) + "가")
    texts += KOREAN_WORDS

    rng = random.Random(20240523)
    for _ in range(600):
        length = rng.randint(2, 5)
        texts.append("".join(chr(0xAC00 + rng.randrange(0, 11172)) for _ in range(length)))

    return [[text, capture(romanise, text, "rr")] for text in texts]


def xsampa_cases() -> list[list[object]]:
    texts = [chr(code) for code in range(0xAC00, 0xD7A4)]
    texts += KOREAN_WORDS + [
        "가",
        "각",
        "갃",
        "힣",
        "사",
        "시",
        "샤",
        "쇼",
        "수",
        "",
        " ",
    ]
    return [[text, capture(hangul2xsampa, text)] for text in texts]


def main() -> None:
    corpus = {
        "pinyin_characters": pinyin_characters(),
        "lazy_pinyin": lazy_pinyin_cases(),
        "pinyin_series": series_cases(),
        "random_lyrics": random_lyrics_cases(),
        "to_romaji": to_romaji_cases(),
        "korean": korean_cases(),
        "korean_xsampa": xsampa_cases(),
    }
    path = TESTDATA / "phoneme_cases.json"
    path.write_text(
        json.dumps(corpus, ensure_ascii=False, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    for name, cases in corpus.items():
        crashed = sum(1 for case in cases if case[-1] is None)
        print(f"{name}: {len(cases)} cases ({crashed} expected failures)")
    print(f"{path}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    sys.exit(main())
