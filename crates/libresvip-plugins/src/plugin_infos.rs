//! Plugin metadata, generated from `libresvip/plugins/*/*.yapsy-plugin`.
//!
//! Regenerate with `python scripts/gen_plugin_infos.py` (see module docs);
//! do not edit by hand.

use libresvip_core::plugin::PluginInfo;

pub const INFO_ACE: PluginInfo = PluginInfo {
    id: "ace",
    name: "ACE Virtual Singer",
    author: "vocoder712",
    website: "https://github.com/SoulMelody/LibreSVIP",
    file_format: "ACE Virtual Singer project",
    description: "Conversion plugin for the raw JSON project files used by ACE Virtual Singer",
    suffixes: &["ace"],
    icon_base64: None,
};
pub const INFO_ACE_VERSION: &str = "1.0.0";

pub const INFO_ACEP: PluginInfo = PluginInfo {
    id: "acep",
    name: "ACE Studio",
    author: "yqzhishen",
    website: "https://space.bilibili.com/102844209",
    file_format: "ACE Studio project File",
    description: "Conversion Plugin for ACE Studio project file",
    suffixes: &["acep", "acet"],
    icon_base64: None,
};
pub const INFO_ACEP_VERSION: &str = "2.0.7";

pub const INFO_AISP: PluginInfo = PluginInfo {
    id: "aisp",
    name: "AISingers",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "AISingers Studio project file",
    description: "Conversion plugin for AISingers Studio project file",
    suffixes: &["aisp"],
    icon_base64: None,
};
pub const INFO_AISP_VERSION: &str = "1.3.9";

pub const INFO_ASS: PluginInfo = PluginInfo {
    id: "ass",
    name: "ASS",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Advanced SubStation Alpha subtitle file",
    description: "Generator plugin for ASS subtitle file",
    suffixes: &["ass"],
    icon_base64: None,
};
pub const INFO_ASS_VERSION: &str = "0.0.1";

pub const INFO_CCS: PluginInfo = PluginInfo {
    id: "ccs",
    name: "CeVIO",
    author: "Sder Colin",
    website: "https://sdercolin.github.io/utaformatix3/",
    file_format: "CeVIO Creative Studio Project file",
    description: "Conversion plugin for CeVIO Creative Studio project file",
    suffixes: &["ccs"],
    icon_base64: None,
};
pub const INFO_CCS_VERSION: &str = "1.0.0";

pub const INFO_DS: PluginInfo = PluginInfo {
    id: "ds",
    name: "DiffSinger",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "DiffSinger parameters file",
    description: "DiffSinger plugin.",
    suffixes: &["ds"],
    icon_base64: None,
};
pub const INFO_DS_VERSION: &str = "0.0.1";

pub const INFO_DSPX: PluginInfo = PluginInfo {
    id: "dspx",
    name: "DSPX",
    author: "OpenVPI",
    website: "https://dspx.diffscope.org/",
    file_format: "DiffScope Project Exchange Format",
    description: "Conversion plugin for DSPX project files",
    suffixes: &["dspx"],
    icon_base64: None,
};
pub const INFO_DSPX_VERSION: &str = "1.0.0";

pub const INFO_DV: PluginInfo = PluginInfo {
    id: "dv",
    name: "DeepVocal",
    author: "Oxygen Dioxide",
    website: "https://space.bilibili.com/340295349",
    file_format: "DeepVocal project file",
    description: "Conversion plugin for dv file",
    suffixes: &["dv", "sk"],
    icon_base64: None,
};
pub const INFO_DV_VERSION: &str = "1.0.0";

pub const INFO_JSON: PluginInfo = PluginInfo {
    id: "json",
    name: "JsonSvip",
    author: "yqzhishen",
    website: "https://space.bilibili.com/102844209",
    file_format: "OpenSVIP file",
    description: "Conversion plugin for OpenSVIP Model, which is based on json",
    suffixes: &["json"],
    icon_base64: None,
};
pub const INFO_JSON_VERSION: &str = "1.0.0";

pub const INFO_LRC: PluginInfo = PluginInfo {
    id: "lrc",
    name: "LRC",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "LRC lyric file",
    description: "Generator plugin for LRC lyric file",
    suffixes: &["lrc"],
    icon_base64: None,
};
pub const INFO_LRC_VERSION: &str = "1.0.0";

pub const INFO_MID: PluginInfo = PluginInfo {
    id: "mid",
    name: "MIDI",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Standard MIDI file (SMF)",
    description: "Conversion plugin for midi file",
    suffixes: &["mid", "midi"],
    icon_base64: None,
};
pub const INFO_MID_VERSION: &str = "1.0.0";

pub const INFO_MTP: PluginInfo = PluginInfo {
    id: "mtp",
    name: "Muta",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Muta project file",
    description: "Conversion plugin for mtp file",
    suffixes: &["mtp"],
    icon_base64: None,
};
pub const INFO_MTP_VERSION: &str = "1.0.0";

pub const INFO_MUSICXML: PluginInfo = PluginInfo {
    id: "musicxml",
    name: "MusicXML",
    author: "Sder Colin",
    website: "https://sdercolin.github.io/utaformatix3/",
    file_format: "MusicXML file",
    description: "Conversion plugin for MusicXML Score file",
    suffixes: &["musicxml", "xml", "mxl"],
    icon_base64: None,
};
pub const INFO_MUSICXML_VERSION: &str = "1.0.0";

pub const INFO_NN: PluginInfo = PluginInfo {
    id: "nn",
    name: "NIAONiao",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "NIAONiao Virtual Singer project file",
    description: "Conversion plugin for NIAONiao project file",
    suffixes: &["nn"],
    icon_base64: None,
};
pub const INFO_NN_VERSION: &str = "1.0.0";

pub const INFO_PPSF: PluginInfo = PluginInfo {
    id: "ppsf",
    name: "Ppsf",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Piapro Studio Song file",
    description: "Conversion plugin for Piapro Studio project file",
    suffixes: &["ppsf"],
    icon_base64: None,
};
pub const INFO_PPSF_VERSION: &str = "1.0.0";

pub const INFO_PS_PROJECT: PluginInfo = PluginInfo {
    id: "ps_project",
    name: "PocketSinger",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Pocket Singer Project file",
    description: "Conversion plugin for Pocket Singer project file",
    suffixes: &["ps_project"],
    icon_base64: None,
};
pub const INFO_PS_PROJECT_VERSION: &str = "1.0.0";

pub const INFO_S5P: PluginInfo = PluginInfo {
    id: "s5p",
    name: "SynthV Editor",
    author: "Sder Colin",
    website: "https://space.bilibili.com/395432/",
    file_format: "Synthesizer V Editor project file",
    description: "Conversion plugin for Synthesizer V Editor project file",
    suffixes: &["s5p"],
    icon_base64: None,
};
pub const INFO_S5P_VERSION: &str = "1.0.0";

pub const INFO_SRT: PluginInfo = PluginInfo {
    id: "srt",
    name: "SRT",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "SubRip subtitle file",
    description: "Generator plugin for SRT subtitle file",
    suffixes: &["srt"],
    icon_base64: None,
};
pub const INFO_SRT_VERSION: &str = "1.0.0";

pub const INFO_SVG: PluginInfo = PluginInfo {
    id: "svg",
    name: "Svg",
    author: "CrSjimo",
    website: "https://sjimo.dev/",
    file_format: "Scalable Vector Graphics",
    description: "Generate graphics containing notes and pitch curve in SVG format. Only the first or specified singing track will be imported.",
    suffixes: &["svg"],
    icon_base64: None,
};
pub const INFO_SVG_VERSION: &str = "1.0.0";

pub const INFO_SVIP: PluginInfo = PluginInfo {
    id: "svip",
    name: "BinSvip",
    author: "yqzhishen",
    website: "https://space.bilibili.com/102844209",
    file_format: "X Studio project file",
    description: "Conversion plugin for the original .svip binary file format",
    suffixes: &["svip"],
    icon_base64: None,
};
pub const INFO_SVIP_VERSION: &str = "1.0.0";

pub const INFO_SVIP3: PluginInfo = PluginInfo {
    id: "svip3",
    name: "Svip3",
    author: "FlutyDeer",
    website: "https://space.bilibili.com/386270936",
    file_format: "Netease Cloud Music XStudio project file",
    description: "Conversion plugin for Netease Cloud Music XStudio project file",
    suffixes: &["svip3"],
    icon_base64: None,
};
pub const INFO_SVIP3_VERSION: &str = "0.0.1";

pub const INFO_SVP: PluginInfo = PluginInfo {
    id: "svp",
    name: "SynthV Studio",
    author: "yqzhishen",
    website: "https://space.bilibili.com/102844209",
    file_format: "Synthesizer V Studio project file",
    description: "Conversion plugin for Synthesizer V Studio project file, it supports reading and writing of all parameters including notes, lyrics, parameter curve, note attributes, note group, and instant pitch mode.\nNotes: Importing a project file with overlapping notes is undefined behavior that can lead to unforeseen exceptions.",
    suffixes: &["svp"],
    icon_base64: None,
};
pub const INFO_SVP_VERSION: &str = "1.11.2";

pub const INFO_TLP: PluginInfo = PluginInfo {
    id: "tlp",
    name: "TuneLab Legacy",
    author: "LiuYunPlayer",
    website: "https://space.bilibili.com/24331207",
    file_format: "TuneLab legacy project file",
    description: "Conversion plugin for TuneLab legacy project file",
    suffixes: &["tlp"],
    icon_base64: None,
};
pub const INFO_TLP_VERSION: &str = "0.5.8";

pub const INFO_TLPX: PluginInfo = PluginInfo {
    id: "tlpx",
    name: "TuneLab",
    author: "LiuYunPlayer",
    website: "https://space.bilibili.com/24331207",
    file_format: "TuneLab project file",
    description: "Conversion plugin for TuneLab project file",
    suffixes: &["tlpx"],
    icon_base64: None,
};
pub const INFO_TLPX_VERSION: &str = "0.1.0";

pub const INFO_TSMSLN: PluginInfo = PluginInfo {
    id: "tsmsln",
    name: "TSMSln",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Techno-Speech Mobile Solution file",
    description: "Conversion plugin for VoiSona Mobile project file",
    suffixes: &["tsmsln"],
    icon_base64: None,
};
pub const INFO_TSMSLN_VERSION: &str = "1.12.1";

pub const INFO_TSSLN: PluginInfo = PluginInfo {
    id: "tssln",
    name: "TSSln",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Techno-Speech Solution file",
    description: "Conversion plugin for VoiSona project file (Only versions 1.8.0 and later are supported)",
    suffixes: &["tssln"],
    icon_base64: None,
};
pub const INFO_TSSLN_VERSION: &str = "1.12.1";

pub const INFO_UFDATA: PluginInfo = PluginInfo {
    id: "ufdata",
    name: "UFData",
    author: "Sder Colin",
    website: "https://sdercolin.github.io/utaformatix3/",
    file_format: "UtaFormatix3 data format",
    description: "Conversion plugin for UFData format",
    suffixes: &["ufdata"],
    icon_base64: None,
};
pub const INFO_UFDATA_VERSION: &str = "1.0.0";

pub const INFO_UST: PluginInfo = PluginInfo {
    id: "ust",
    name: "Ust",
    author: "Sder Colin",
    website: "https://sdercolin.github.io/utaformatix3/",
    file_format: "UTAU Sequence Text",
    description: "Conversion plugin for UST file format",
    suffixes: &["ust"],
    icon_base64: None,
};
pub const INFO_UST_VERSION: &str = "1.0.0";

pub const INFO_USTX: PluginInfo = PluginInfo {
    id: "ustx",
    name: "Ustx",
    author: "Oxygen Dioxide",
    website: "https://space.bilibili.com/340295349",
    file_format: "OpenUtau project file",
    description: "Conversion plugin for Ustx file format",
    suffixes: &["ustx"],
    icon_base64: None,
};
pub const INFO_USTX_VERSION: &str = "1.0.0";

pub const INFO_VFP: PluginInfo = PluginInfo {
    id: "vfp",
    name: "VOX Factory",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "VOX Factory Project File",
    description: "Conversion plugin for VOX Factory Project File",
    suffixes: &["vfp"],
    icon_base64: None,
};
pub const INFO_VFP_VERSION: &str = "1.0.0";

pub const INFO_VOG: PluginInfo = PluginInfo {
    id: "vog",
    name: "Vogen",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Vogen project file",
    description: "Conversion plugin for Vogen project file",
    suffixes: &["vog"],
    icon_base64: None,
};
pub const INFO_VOG_VERSION: &str = "1.0.0";

pub const INFO_VPR: PluginInfo = PluginInfo {
    id: "vpr",
    name: "Vpr",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "New Vocaloid Project file",
    description: "Conversion plugin for New Vocaloid project file",
    suffixes: &["vpr"],
    icon_base64: None,
};
pub const INFO_VPR_VERSION: &str = "1.0.0";

pub const INFO_VSHP: PluginInfo = PluginInfo {
    id: "vshp",
    name: "VocalShifter",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "VocalShifter project file",
    description: "Import parameters from VocalShifter project file.",
    suffixes: &["vshp"],
    icon_base64: None,
};
pub const INFO_VSHP_VERSION: &str = "1.0.0";

pub const INFO_VSPX: PluginInfo = PluginInfo {
    id: "vspx",
    name: "VocalSharp",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "VocalSharp project file",
    description: "Conversion plugin for VocalSharp project file",
    suffixes: &["vspx"],
    icon_base64: None,
};
pub const INFO_VSPX_VERSION: &str = "1.0.0";

pub const INFO_VSQ: PluginInfo = PluginInfo {
    id: "vsq",
    name: "Vsq",
    author: "Selio Jiang",
    website: "http://selio.me/",
    file_format: "Vocaloid 2 Sequence file",
    description: "Conversion plugin for Vocaloid 2 sequence file",
    suffixes: &["vsq"],
    icon_base64: None,
};
pub const INFO_VSQ_VERSION: &str = "1.0.0";

pub const INFO_VSQX: PluginInfo = PluginInfo {
    id: "vsqx",
    name: "Vsqx",
    author: "Sder Colin",
    website: "https://sdercolin.github.io/utaformatix3/",
    file_format: "Vocaloid Sequence XML file",
    description: "Conversion plugin for Vocaloid Sequence XML file",
    suffixes: &["vsqx"],
    icon_base64: None,
};
pub const INFO_VSQX_VERSION: &str = "1.0.0";

pub const INFO_VVPROJ: PluginInfo = PluginInfo {
    id: "vvproj",
    name: "VVProj",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "VOICEVOX Project file",
    description: "Conversion plugin for VOICEVOX Project file",
    suffixes: &["vvproj"],
    icon_base64: None,
};
pub const INFO_VVPROJ_VERSION: &str = "1.0.0";

pub const INFO_VXF: PluginInfo = PluginInfo {
    id: "vxf",
    name: "VX-β",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "VX-β project file",
    description: "Conversion plugin for VX-β project file",
    suffixes: &["vxf"],
    icon_base64: None,
};
pub const INFO_VXF_VERSION: &str = "3.0.2";

pub const INFO_XVSQ: PluginInfo = PluginInfo {
    id: "xvsq",
    name: "Cadencii",
    author: "SoulMelody",
    website: "https://space.bilibili.com/175862486",
    file_format: "Cadencii project file",
    description: "Conversion plugin for Cadencii project file",
    suffixes: &["xvsq"],
    icon_base64: None,
};
pub const INFO_XVSQ_VERSION: &str = "1.0.0";

pub const INFO_Y77: PluginInfo = PluginInfo {
    id: "y77",
    name: "Y77",
    author: "FlutyDeer",
    website: "https://space.bilibili.com/386270936",
    file_format: "Yuan77 project file",
    description: "Conversion plugin for Yuan77 project file",
    suffixes: &["y77"],
    icon_base64: None,
};
pub const INFO_Y77_VERSION: &str = "1.0.0";
