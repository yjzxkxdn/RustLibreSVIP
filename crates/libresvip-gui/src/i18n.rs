//! UI translations.
//!
//! The Chinese text comes from LibreSVIP's own gettext catalog
//! (`res/locales/zh_CN/LC_MESSAGES/libresvip.mo`, embedded under
//! `assets/locales`) with a hand-written table for the strings the QML
//! catalog does not cover. Japanese and German use the same upstream
//! catalogs and fall back to English for untranslated entries.

use libresvip_core::core::config::Language;
use libresvip_core::utils::translation::Translation;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    ZhCn,
    EnUs,
    JaJp,
    DeDe,
}

impl Lang {
    pub fn from_settings(value: Language) -> Self {
        match value {
            Language::Chinese => Lang::ZhCn,
            Language::English => Lang::EnUs,
            Language::Japanese => Lang::JaJp,
            Language::German => Lang::DeDe,
        }
    }

    pub fn to_settings(self) -> Language {
        match self {
            Lang::ZhCn => Language::Chinese,
            Lang::EnUs => Language::English,
            Lang::JaJp => Language::Japanese,
            Lang::DeDe => Language::German,
        }
    }

    #[allow(dead_code)]
    pub fn code(self) -> &'static str {
        match self {
            Lang::ZhCn => "zh_CN",
            Lang::EnUs => "en_US",
            Lang::JaJp => "ja_JP",
            Lang::DeDe => "de_DE",
        }
    }

    /// Label shown in the language menu (each language names itself).
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::ZhCn => "简体中文",
            Lang::EnUs => "English",
            Lang::JaJp => "日本語",
            Lang::DeDe => "Deutsch",
        }
    }

    pub fn all() -> [Lang; 4] {
        [Lang::ZhCn, Lang::EnUs, Lang::JaJp, Lang::DeDe]
    }
}

pub struct Translator {
    pub lang: Lang,
    catalog: Option<Translation>,
}

const ZH_MO: &[u8] = include_bytes!("../assets/locales/zh_CN.mo");
const JA_MO: &[u8] = include_bytes!("../assets/locales/ja_JP.mo");
const DE_MO: &[u8] = include_bytes!("../assets/locales/de_DE.mo");

impl Translator {
    pub fn new(lang: Lang) -> Self {
        let catalog = match lang {
            Lang::EnUs => None,
            Lang::ZhCn => Translation::load_mo(ZH_MO),
            Lang::JaJp => Translation::load_mo(JA_MO),
            Lang::DeDe => Translation::load_mo(DE_MO),
        };
        Self { lang, catalog }
    }

    /// Translates a UI string. `en` is the English source string (identical to
    /// the `qsTr()` msgid in the QML files).
    pub fn t(&self, en: &'static str) -> String {
        if self.lang == Lang::EnUs {
            return en.to_owned();
        }
        if let Some(text) = self.catalog_lookup(en) {
            return text;
        }
        if self.lang == Lang::ZhCn {
            if let Some(text) = zh_table(en) {
                return text.to_owned();
            }
        }
        en.to_owned()
    }

    /// Translates a runtime string (plugin names, descriptions, enum titles).
    pub fn t_dyn(&self, text: &str) -> String {
        if self.lang == Lang::EnUs || text.is_empty() {
            return text.to_owned();
        }
        self.catalog_lookup(text).unwrap_or_else(|| text.to_owned())
    }

    fn catalog_lookup(&self, text: &str) -> Option<String> {
        let catalog = self.catalog.as_ref()?;
        let translated = catalog.gettext(text);
        if translated.is_empty() || translated == text {
            None
        } else {
            Some(translated)
        }
    }
}

/// Strings the upstream `.mo` catalog does not contain (mostly newer QML
/// labels); mirroring the terminology used by the translated ones.
fn zh_table(en: &str) -> Option<&'static str> {
    Some(match en {
        "LibreSVIP" => "LibreSVIP",
        "Rust port of LibreSVIP, with design elements from UtaFormatix3" => {
            "LibreSVIP 的 Rust 移植，并吸收了 UtaFormatix3 的设计"
        }
        "Based on LibreSVIP 2.9.2 (MIT) and UtaFormatix3 (Apache-2.0; design and algorithms only)" => {
            "基于 LibreSVIP 2.9.2（MIT）与 UtaFormatix3（Apache-2.0，仅借鉴设计与算法）"
        }
        "SVS Projects Converter" => "歌声合成工程转换器",
        "Convert (&C)" => "转换(&C)",
        "Formats (&F)" => "格式(&F)",
        "Settings (&S)" => "设置(&S)",
        "Help (&H)" => "帮助(&H)",
        "Minimize" => "最小化",
        "Maximize or Restore" => "最大化或还原",
        "Close" => "关闭",
        "Import Projects (Ctrl+O)" => "导入工程(Ctrl+O)",
        "Perform All Tasks (Ctrl+Enter)" => "执行所有任务(Ctrl+Enter)",
        "Clear Tasks (Ctrl+R)" => "清空任务(Ctrl+R)",
        "Swap Input and Output (Ctrl+Tab)" => "交换输入输出(Ctrl+Tab)",
        "Input Format (&I)" => "输入格式(&I)",
        "Output Format (&E)" => "输出格式(&E)",
        "Options (&O)" => "选项(&O)",
        "Themes (&T)" => "主题(&T)",
        "Light" => "浅色",
        "Dark" => "深色",
        "System" => "跟随系统",
        "Language (&L)" => "语言(&L)",
        "About (Ctrl+A)" => "关于(Ctrl+A)",
        "Check for Updates" => "检查更新",
        "Documentation (F1)" => "文档(F1)",
        "Reset Tasks When Changing Input" => "切换输入时重置任务",
        "Input Format: " => "输入格式：",
        "Output Format: " => "输出格式：",
        "Auto-Detect Input File Type" => "自动检测输入文件类型",
        "Set Output File Extension Automatically" => "自动设置输出文件扩展名",
        "View Detail Information" => "查看详细信息",
        "Swap Input and Output" => "交换输入输出",
        "Drag and drop files here" => "拖放文件到此处",
        "Release to add files" => "松开以添加文件",
        "or" => "或",
        "Browse Files" => "浏览文件",
        "Direct Mode" => "直接模式",
        "Singing Track Merging Mode" => "歌声音轨合并模式",
        "Singing Track Grouping Mode" => "歌声音轨分组模式",
        "Task List" => "任务列表",
        "Remove" => "移除",
        "Max Track count:" => "最大音轨数：",
        "Continue Adding files" => "继续添加文件",
        "Clear Task List" => "清空任务列表",
        "Reset Default Extension" => "重置默认扩展名",
        "Remove Tasks With Other Extensions" => "移除其他扩展名的任务",
        "Expand or Collapse Task Toolbar" => "展开或收起任务工具栏",
        "File successfully converted" => "文件转换成功",
        "Open" => "打开",
        "Open folder" => "打开文件夹",
        "File skipped due to conflict" => "文件因冲突被跳过",
        "Show warning details" => "查看警告详情",
        "This project file may contain abnormal or illegal data" => {
            "该工程文件可能包含异常或非法数据"
        }
        "Show error details" => "查看错误详情",
        "File failed to convert, click to view the error message" => {
            "文件转换失败，点击查看错误信息"
        }
        "File failed to convert, below is the error message:" => "文件转换失败，错误信息如下：",
        "Copy error message" => "复制错误信息",
        "Copy warning message" => "复制警告信息",
        "Advanced Settings" => "高级选项",
        "Input Options" => "输入选项",
        "Output Options" => "输出选项",
        "Import as" => "导入为",
        "Export to" => "导出为",
        "Help" => "帮助",
        "Open Output Folder When Done" => "完成后打开输出文件夹",
        "Choose Output Folder" => "选择输出文件夹",
        "Output Folder" => "输出文件夹",
        "Deal with Conflicts" => "冲突处理",
        "Converting" => "转换中",
        "Prompt" => "询问",
        "Overwrite" => "覆盖",
        "Skip" => "跳过",
        "About" => "关于",
        "Version: " => "版本：",
        "Author: SoulMelody" => "作者：SoulMelody",
        "Author's Profile" => "作者主页",
        "Repo URL" => "仓库地址",
        "Do you want to overwrite the file?" => "要覆盖该文件吗？",
        "Yes" => "是",
        "No" => "否",
        "Yes to All" => "全部是",
        "No to All" => "全部否",
        "Cancel" => "取消",
        "OK" => "确定",
        "Settings" => "设置",
        "Basic Settings" => "基础设置",
        "Save Path Settings" => "保存路径设置",
        "Format Provider Plugins" => "格式支持插件",
        "Lyric Replacement Rules" => "歌词替换规则",
        "Updates Settings" => "更新设置",
        "Conversion Settings" => "转换设置",
        "Multi-Threaded Conversion" => "多线程转换",
        "Reset Task List When Changing Input File Type" => "切换输入文件类型时重置任务列表",
        "Same as Source" => "与源文件相同",
        "Desktop" => "桌面",
        "Preset Folder" => "预设文件夹",
        "Custom (Browse ...)" => "自定义(浏览…)",
        "Manage Folders Presets" => "管理文件夹预设",
        "Custom folder" => "自定义文件夹",
        "Folder Presets" => "文件夹预设",
        "Click to add a folder preset" => "点击添加文件夹预设",
        "Auto Check for Updates" => "自动检查更新",
        "Please select input and output formats first." => "请先选择输入和输出格式。",
        "No tasks" => "暂无任务",
        "The path does not exist or is not a directory." => "路径不存在或不是文件夹。",
        "Plugins" => "插件",
        "Enabled" => "已启用",
        "Theme" => "主题",
        "Language" => "语言",
        "Save" => "保存",
        "Reset" => "重置",
        "Browse ..." => "浏览…",
        "Select Folder" => "选择文件夹",
        "Added file" => "已添加文件",
        "This build of LibreSVIP (Rust port) cannot check for updates online." => {
            "此版本(LibreSVIP Rust 移植版)无法在线检查更新。"
        }
        "Open Project Homepage" => "打开项目主页",
        "LibreSVIP is an open-sourced, liberal and extensionable framework that can convert your singing synthesis projects between different file formats." => {
            "LibreSVIP 是一个开源、自由、可扩展的框架，可以在不同文件格式之间转换你的歌声合成工程。"
        }
        "All people should have the right and freedom to choose. That's why we're committed to giving you a second chance to keep your creations free from the constraints of platforms and coterie." => {
            "所有人都应有选择的权利与自由。因此我们致力于给你第二次机会，让你的创作不再受平台与小圈子的束缚。"
        }
        "Rust port" => "Rust 移植版",
        "Write the syllable (phoneme) layer" => "写入音节（音素）层",
        "This program is running in a restricted (sandboxed) process and cannot write files. Start it normally and trust it in your security software." => {
            "当前程序运行在受限（沙箱）进程中，无法写入文件。请正常启动本程序，并在安全软件中将其设为信任。"
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_passed_through() {
        let translator = Translator::new(Lang::EnUs);
        assert_eq!(translator.t("Select File Formats"), "Select File Formats");
        assert_eq!(translator.t_dyn("Pitch Shift"), "Pitch Shift");
    }

    #[test]
    fn chinese_catalog_covers_gui_and_plugin_strings() {
        let translator = Translator::new(Lang::ZhCn);
        // GUI strings: gettext catalog or the hand-written table.
        assert_eq!(translator.t("Select File Formats"), "格式选择");
        assert_eq!(translator.t("Output Folder"), "输出文件夹");
        assert_eq!(translator.t("Import as"), "导入为");
        assert_eq!(translator.t("Export to"), "导出为");
        // Plugin and middleware strings live in the merged plugin catalogs.
        assert_eq!(translator.t_dyn("Pitch Shift"), "音高变调");
        assert_eq!(translator.t_dyn("ACE Studio project File"), "ACE Studio 工程文件");
        assert_eq!(translator.t_dyn("OpenUtau project file"), "OpenUtau 工程文件");
        assert_eq!(translator.t_dyn("Plus sign handling mode"), "加号处理模式");
    }

    #[test]
    fn japanese_and_german_come_from_the_merged_catalogs() {
        let japanese = Translator::new(Lang::JaJp);
        assert_eq!(japanese.t_dyn("Pitch Shift"), "ピッチシフト");
        assert_ne!(japanese.t_dyn("Project Zoom"), "Project Zoom");
        let german = Translator::new(Lang::DeDe);
        assert_eq!(
            german.t_dyn("Plus sign handling mode"),
            "Modus für die Behandlung des Pluszeichens"
        );
    }

    #[test]
    fn unknown_strings_fall_back_to_english() {
        let translator = Translator::new(Lang::ZhCn);
        assert_eq!(translator.t("some string that is not translated"), "some string that is not translated");
        assert_eq!(translator.t_dyn(""), "");
    }
}
