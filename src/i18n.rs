use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::HashMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    System,
    English,
    Chinese,
}
impl Language {
    pub fn chinese(self, locale: &str) -> bool {
        match self {
            Self::Chinese => true,
            Self::English => false,
            Self::System => locale
                .split(['-', '_', '.', '@'])
                .next()
                .is_some_and(|l| l.eq_ignore_ascii_case("zh")),
        }
    }
}
static CHINESE: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/locales/zh-CN.json")).expect("Chinese catalog")
});
pub fn translate(chinese: bool, text: &str) -> Cow<'_, str> {
    if chinese {
        if let Some(value) = CHINESE.get(text) {
            return Cow::Borrowed(value);
        }
    }
    Cow::Borrowed(text)
}
pub fn use_language(ctx: &egui::Context, language: Language, locale: &str) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("mori.language"), language.chinese(locale)));
}
pub fn tr(ctx: &egui::Context, text: impl AsRef<str>) -> String {
    let chinese = ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("mori.language"))
            .unwrap_or(false)
    });
    translate(chinese, text.as_ref()).into_owned()
}
pub fn system_locale() -> String {
    if let Ok(locale) = std::env::var("MORI_SYSTEM_LOCALE") {
        return locale;
    }
    #[cfg(target_os = "macos")]
    {
        objc2_foundation::NSLocale::preferredLanguages()
            .firstObject()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "en".into())
    }
    #[cfg(target_os = "windows")]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultLocaleName(name: *mut u16, size: i32) -> i32;
        }
        let mut buffer = [0_u16; 85];
        let len = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
        if len > 1 {
            String::from_utf16_lossy(&buffer[..len as usize - 1])
        } else {
            "en".into()
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        for name in ["LC_ALL", "LC_MESSAGES", "LANG"] {
            if let Ok(locale) = std::env::var(name) {
                if !locale.is_empty() {
                    return locale;
                }
            }
        }
        "en".into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn language_follows_system_and_allows_an_explicit_override() {
        for locale in ["zh_CN.UTF-8", "zh-Hans-CN", "zh_TW"] {
            assert!(Language::System.chinese(locale));
            assert!(!Language::English.chinese(locale));
        }
        assert!(!Language::System.chinese("en_US.UTF-8"));
        assert!(!Language::System.chinese("de-DE"));
        assert!(Language::Chinese.chinese("en-US"));
    }
    #[test]
    fn both_catalogs_preserve_unknown_user_content() {
        assert_eq!(translate(true, "Settings"), "设置");
        assert_eq!(translate(false, "Settings"), "Settings");
        assert_eq!(translate(true, "My drawing 17"), "My drawing 17");
    }
}
