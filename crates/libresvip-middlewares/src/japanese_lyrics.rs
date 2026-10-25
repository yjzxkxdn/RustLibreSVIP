//! Japanese lyrics conversion middleware (T6 of the UtaFormatix3 absorption
//! plan). Converts note lyrics between kana/romaji × CV/VCV conventions.
//!
//! Rust-port addition — Python LibreSVIP has no equivalent. Opt-in, so
//! default conversions are untouched.

use libresvip_core::core::warning_types::{show_warning_kind, WarningKind};
use libresvip_core::error::Result;
use libresvip_core::model::base::Project;
use libresvip_core::options::{EnumVariant, OptionFieldDef, Options};
use libresvip_core::plugin::{Middleware, PluginInfo};
use libresvip_core::semantics::lyrics::{
    convert_japanese_lyrics, detect_lyrics_type, JapaneseLyricsType, LyricsTarget,
};

pub static INFO_JAPANESE_LYRICS: PluginInfo = PluginInfo {
    id: "japanese_lyrics",
    name: "Japanese Lyrics Conversion",
    author: "依旧在星空下等你",
    website: "https://github.com/yjzxkxdn/RustLibreSVIP",
    file_format: "",
    description: "Convert Japanese lyrics between kana/romaji and CV/VCV conventions. Tracks whose lyrics type cannot be detected (70% consistency required) are left unchanged.",
    suffixes: &[],
    icon_base64: None,
};

const TARGET_VARIANTS: &[EnumVariant] = &[
    EnumVariant { name: "KANA_CV", value: "kana-cv", title: "Kana CV" },
    EnumVariant { name: "KANA_VCV", value: "kana-vcv", title: "Kana VCV" },
    EnumVariant { name: "ROMAJI_CV", value: "romaji-cv", title: "Romaji CV" },
    EnumVariant { name: "ROMAJI_VCV", value: "romaji-vcv", title: "Romaji VCV" },
];

pub struct JapaneseLyricsMiddleware;

impl Middleware for JapaneseLyricsMiddleware {
    fn info(&self) -> &'static PluginInfo {
        &INFO_JAPANESE_LYRICS
    }

    fn process_options(&self) -> Vec<OptionFieldDef> {
        vec![
            OptionFieldDef::enumerated(
                "target",
                "Target lyrics type",
                "LyricsTarget",
                TARGET_VARIANTS,
                "ROMAJI_CV",
            )
                .with_description(
                    "Convention to rewrite the lyrics into; tracks that do not \
                     match any convention are skipped with a warning.",
                ),
            OptionFieldDef::bool("include_unknown", "Convert undetected tracks too", false)
                .with_description(
                    "Treat lyrics that could not be detected as romaji CV and \
                     convert them anyway.",
                ),
        ]
    }

    fn process(&self, mut project: Project, options: &Options) -> Result<Project> {
        let Some(target) = LyricsTarget::parse(&options.str("target", "romaji-cv")) else {
            return Err(libresvip_core::LibreSvipError::msg(format!(
                "unknown lyrics target: {}",
                options.str("target", "romaji-cv")
            )));
        };
        let include_unknown = options.bool("include_unknown", false);
        for track in &mut project.track_list {
            if let libresvip_core::model::base::Track::Singing(singing) = track {
                let detected = detect_lyrics_type(
                    &singing
                        .note_list
                        .iter()
                        .map(|note| note.lyric.clone())
                        .collect::<Vec<_>>(),
                );
                match detected {
                    JapaneseLyricsType::Unknown if !include_unknown => {
                        show_warning_kind(
                            WarningKind::FallbackApplied,
                            format!(
                                "track {:?}: lyrics type not detected, left unchanged",
                                singing.title
                            ),
                        );
                        continue;
                    }
                    detected => {
                        if detected == JapaneseLyricsType::Unknown {
                            show_warning_kind(
                                WarningKind::FallbackApplied,
                                format!(
                                    "track {:?}: lyrics type not detected, treated as romaji CV",
                                    singing.title
                                ),
                            );
                        }
                        let converted = convert_japanese_lyrics(
                            &singing
                                .note_list
                                .iter()
                                .map(|note| note.lyric.clone())
                                .collect::<Vec<_>>(),
                            target,
                        );
                        for (note, lyric) in singing.note_list.iter_mut().zip(converted) {
                            note.lyric = lyric;
                        }
                    }
                }
            }
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libresvip_core::model::base::{Note, SingingTrack, Track};
    use libresvip_core::options::OptionValue;

    fn project_with_lyrics(list: &[&str]) -> Project {
        Project {
            track_list: vec![Track::Singing(SingingTrack {
                note_list: list
                    .iter()
                    .map(|&lyric| Note {
                        lyric: lyric.to_owned(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })],
            ..Default::default()
        }
    }

    fn options(pairs: &[(&str, OptionValue)]) -> Options {
        Options::from_pairs(pairs.iter().map(|(key, value)| ((*key).to_owned(), value.clone())))
    }

    #[test]
    fn converts_detected_tracks_and_skips_unknown() {
        let mut project = project_with_lyrics(&["a ka", "a ri", "n ga"]);
        let opts = options(&[("target", OptionValue::Str("kana-cv".to_owned()))]);
        project = JapaneseLyricsMiddleware.process(project, &opts).expect("process");
        let Track::Singing(singing) = &project.track_list[0] else {
            panic!("singing track");
        };
        let lyrics: Vec<&str> = singing.note_list.iter().map(|n| n.lyric.as_str()).collect();
        assert_eq!(lyrics, vec!["か", "り", "が"]);
    }

    #[test]
    fn unknown_tracks_are_skipped_with_warning() {
        let mut project = project_with_lyrics(&["你", "好"]);
        let opts = options(&[]);
        project = JapaneseLyricsMiddleware.process(project, &opts).expect("process");
        let Track::Singing(singing) = &project.track_list[0] else {
            panic!("singing track");
        };
        assert_eq!(singing.note_list[0].lyric, "你");
        let typed = libresvip_core::core::warning_types::take_typed_warnings();
        assert_eq!(typed.len(), 1, "a skip warning was recorded");
    }
}
