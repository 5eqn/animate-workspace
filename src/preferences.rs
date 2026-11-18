use crate::model::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushDefaults {
    pub ink: f32,
    pub liquify: f32,
}
impl Default for BrushDefaults {
    fn default() -> Self {
        Self {
            ink: 42.0,
            liquify: 240.0,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputPreferences {
    pub width: u32,
    pub fps: u32,
    pub crf: u32,
    pub audio: bool,
}
impl Default for OutputPreferences {
    fn default() -> Self {
        Self {
            width: 1920,
            fps: 60,
            crf: 18,
            audio: true,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub onion: Onion,
    pub pressure_gamma: f32,
    pub minimum_pressure: f32,
    pub spacing: f32,
    pub interpolation: Interpolation,
    pub liquify_strength: f32,
    pub bindings: Vec<Binding>,
    /// Positive means the picture is delayed relative to the device's audio clock.
    pub video_delay_ms: f64,
    pub brush_defaults: [BrushDefaults; 6],
    pub default_aspect: f32,
    pub default_step: f64,
    pub export: OutputPreferences,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            onion: Onion::default(),
            pressure_gamma: 1.0,
            minimum_pressure: 0.05,
            spacing: 1.5,
            interpolation: Interpolation::Smooth,
            liquify_strength: 0.65,
            bindings: default_bindings(),
            video_delay_ms: 0.0,
            brush_defaults: [6.0, 42.0, 42.0, 42.0, 100.0, 140.0].map(|ink| BrushDefaults {
                ink,
                liquify: 240.0,
            }),
            default_aspect: 2.39,
            default_step: 0.125,
            export: OutputPreferences::default(),
        }
    }
}
impl Preferences {
    pub fn layer(&self, name: &str, blend: Blend) -> Layer {
        let defaults = &self.brush_defaults[blend.shader_id() as usize];
        let mut layer = Layer::new(name, blend, defaults.ink);
        layer.liquify_width = defaults.liquify;
        layer
    }
    pub fn new_document(&self) -> Document {
        let mut document = Document::default();
        document.settings.aspect = self.default_aspect;
        document.timing.step = self.default_step;
        self.apply_output(&mut document.settings.export);
        document.settings.export.start = 0.0;
        document.settings.export.end = document.timing.duration;
        document.layers = vec![
            self.layer("Paper", Blend::Base),
            self.layer("Ink", Blend::Xor),
            self.layer("Movement", Blend::Move),
            self.layer("Atmosphere", Blend::Blur),
        ];
        document
    }
    pub fn apply_output(&self, output: &mut ExportSettings) {
        output.width = self.export.width;
        output.fps = self.export.fps;
        output.crf = self.export.crf;
        output.audio = self.export.audio;
    }
    pub fn visual_time(&self, audio_time: f64) -> f64 {
        (audio_time - self.video_delay_ms / 1000.0).max(0.0)
    }
    pub fn valid(&self) -> bool {
        self.video_delay_ms.is_finite()
            && self.pressure_gamma.is_finite()
            && self.pressure_gamma > 0.0
            && self.minimum_pressure.is_finite()
            && (0.0..=1.0).contains(&self.minimum_pressure)
            && self.spacing.is_finite()
            && self.spacing > 0.0
            && self.liquify_strength.is_finite()
            && self.liquify_strength > 0.0
            && self.default_aspect.is_finite()
            && self.default_aspect > 0.0
            && self.default_step.is_finite()
            && self.default_step > 0.0
            && self.export.width >= 2
            && self.export.fps > 0
            && self.export.crf <= 51
            && self
                .brush_defaults
                .iter()
                .all(|b| [b.ink, b.liquify].iter().all(|v| v.is_finite() && *v > 0.0))
            && self.onion.opacity.is_finite()
            && (0.0..=1.0).contains(&self.onion.opacity)
            && self.onion.falloff.is_finite()
            && (0.0..=1.0).contains(&self.onion.falloff)
            && self
                .bindings
                .iter()
                .all(|b| eframe::egui::Key::from_name(&b.key).is_some())
    }
}
pub struct PreferenceStore {
    pub path: PathBuf,
    last: Vec<u8>,
}
impl PreferenceStore {
    pub fn open() -> (Self, Preferences, Option<String>) {
        Self::at(config_path())
    }
    pub fn at(path: PathBuf) -> (Self, Preferences, Option<String>) {
        let mut warning = None;
        let preferences = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Preferences>(&bytes) {
                Ok(p) if p.valid() => p,
                _ => {
                    warning = Some("Could not read preferences".into());
                    Preferences::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Preferences::default(),
            Err(e) => {
                warning = Some(format!("Preferences: {e}"));
                Preferences::default()
            }
        };
        let last = serde_json::to_vec(&preferences).unwrap();
        (Self { path, last }, preferences, warning)
    }
    pub fn save_if_changed(&mut self, preferences: &Preferences) -> Result<(), String> {
        let bytes = serde_json::to_vec(preferences).map_err(|e| e.to_string())?;
        if bytes == self.last {
            return Ok(());
        }
        if !preferences.valid() {
            return Err("Invalid preferences".into());
        }
        let parent = self.path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.as_file().sync_all().map_err(|e| e.to_string())?;
        file.persist(&self.path).map_err(|e| e.to_string())?;
        self.last = bytes;
        Ok(())
    }
}
fn config_path() -> PathBuf {
    if let Some(path) = std::env::var_os("MORI_PREFERENCES_PATH") {
        return path.into();
    }
    #[cfg(target_os = "macos")]
    let root = PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/Application Support");
    #[cfg(target_os = "windows")]
    let root = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default());
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    root.join("MORI/preferences.json")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_survive_restart_and_remain_outside_document_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings/preferences.json");
        let (mut store, mut prefs, _) = PreferenceStore::at(path.clone());
        prefs.video_delay_ms = -73.0;
        prefs.brush_defaults[1].liquify = 420.0;
        prefs.onion.enabled = false;
        store.save_if_changed(&prefs).unwrap();
        let (_, loaded, warning) = PreferenceStore::at(path);
        assert!(warning.is_none());
        assert_eq!(loaded.video_delay_ms, -73.0);
        assert_eq!(loaded.layer("Ink", Blend::Xor).liquify_width, 420.0);
        let mut doc = loaded.new_document();
        let mut history = History::default();
        history.record(&doc);
        doc.settings.aspect = 1.0;
        history.undo(&mut doc);
        assert!(!loaded.onion.enabled);
        let encoded = serde_json::to_value(&doc).unwrap();
        assert!(!encoded.to_string().contains("onion"));
        assert!(!encoded["settings"]["export"]
            .as_object()
            .unwrap()
            .contains_key("fps"));
        assert!(!encoded["settings"]["export"]
            .as_object()
            .unwrap()
            .contains_key("width"));
        assert_eq!(encoded["settings"]["export"]["end"], 8.0);
    }
    #[test]
    fn signed_delay_maps_the_same_audio_clock_in_both_directions() {
        let mut p = Preferences {
            video_delay_ms: 125.0,
            ..Default::default()
        };
        assert_eq!(p.visual_time(1.0), 0.875);
        p.video_delay_ms = -125.0;
        assert_eq!(p.visual_time(1.0), 1.125);
        assert_eq!(p.visual_time(0.0), 0.125);
    }
    #[test]
    fn opening_a_project_does_not_replace_global_brush_or_key_preferences() {
        let p = Preferences {
            spacing: 3.0,
            ..Default::default()
        };
        let mut document = p.new_document();
        document.layers[0].width = 80.0;
        let saved = serde_json::to_string(&document).unwrap();
        let opened: Document = serde_json::from_str(&saved).unwrap();
        assert_eq!(opened.layers[0].width, 80.0);
        assert_eq!(p.new_document().layers[0].width, 6.0);
        assert_eq!(p.spacing, 3.0);
    }
}
