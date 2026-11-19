use crate::{
    i18n::{self, Language},
    model::*,
};
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
    pub marker: f32,
    pub eraser: f32,
}
impl Default for BrushDefaults {
    fn default() -> Self {
        Self {
            ink: 42.0,
            liquify: 240.0,
            marker: 42.0,
            eraser: 42.0,
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PauseBehavior {
    #[default]
    ReturnToStart,
    KeepPlayhead,
}
impl PauseBehavior {
    pub fn position(self, timing: &Timing, start: f64, current: f64) -> f64 {
        timing.snap(match self {
            Self::ReturnToStart => start,
            Self::KeepPlayhead => current,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub language: Language,
    pub onion: Onion,
    pub pressure_gamma: f32,
    pub minimum_pressure: f32,
    pub spacing: f32,
    pub interpolation: Interpolation,
    pub liquify_strength: f32,
    pub bindings: Vec<Binding>,
    /// Positive means the picture is delayed relative to the device's audio clock.
    pub video_delay_ms: f64,
    pub pause_behavior: PauseBehavior,
    pub timeline_follow: bool,
    pub brush_defaults: [BrushDefaults; 6],
    pub default_aspect: f32,
    pub default_step: f64,
    pub default_watermark: bool,
    pub export: OutputPreferences,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::System,
            onion: Onion::default(),
            pressure_gamma: 1.0,
            minimum_pressure: 0.0,
            spacing: 1.5,
            interpolation: Interpolation::Smooth,
            liquify_strength: 0.65,
            bindings: default_bindings(),
            video_delay_ms: -64.0,
            pause_behavior: PauseBehavior::default(),
            timeline_follow: true,
            brush_defaults: [6.0, 42.0, 42.0, 42.0, 100.0, 140.0].map(|ink| BrushDefaults {
                ink,
                liquify: 240.0,
                marker: 42.0,
                eraser: 42.0,
            }),
            default_aspect: 2.39,
            default_step: 0.125,
            default_watermark: true,
            export: OutputPreferences::default(),
        }
    }
}
impl Preferences {
    pub fn migrate_shortcuts(&mut self) {
        let legacy = |action: &str, key: &str| {
            self.bindings.iter().any(|b| {
                b.action == action && b.key == key && !b.command && !b.control && !b.alt && !b.shift
            })
        };
        if legacy("Undo", "Z") && legacy("Redo", "Y") {
            let defaults = default_bindings();
            for action in ["Undo", "Redo"] {
                let candidate = defaults.iter().find(|b| b.action == action).unwrap();
                if self.bindings.iter().any(|b| {
                    b.action != action
                        && b.key == candidate.key
                        && b.command == candidate.command
                        && b.control == candidate.control
                        && b.alt == candidate.alt
                        && b.shift == candidate.shift
                }) {
                    return;
                }
            }
            for action in ["Undo", "Redo"] {
                *self
                    .bindings
                    .iter_mut()
                    .find(|b| b.action == action)
                    .unwrap() = defaults
                    .iter()
                    .find(|b| b.action == action)
                    .unwrap()
                    .clone();
            }
        }
    }
    pub fn layer(&self, name: &str, blend: Blend) -> Layer {
        let defaults = &self.brush_defaults[blend.shader_id() as usize];
        let mut layer = Layer::new(name, blend, defaults.ink);
        layer.liquify_width = defaults.liquify;
        layer.marker_width = defaults.marker;
        layer.eraser_width = defaults.eraser;
        layer
    }
    pub fn new_document(&self) -> Document {
        let mut document = Document::default();
        document.settings.aspect = self.default_aspect;
        document.settings.watermark = self.default_watermark;
        document.timing.tempos[0].division = 1.0 / self.default_step;
        self.apply_output(&mut document.settings.export);
        document.settings.export.start = 0.0;
        document.settings.export.end = document.timing.duration;
        document.layers = vec![
            {
                let mut layer = self.layer("Paper", Blend::Base);
                layer.blend = Blend::Xor;
                layer
            },
            {
                let mut layer = self.layer("Ink", Blend::Xor);
                layer.brush = Brush::Marker;
                layer
            },
            self.layer("Movement", Blend::Move),
            self.layer("Atmosphere", Blend::Blur),
        ];
        if self.language.chinese(&i18n::system_locale()) {
            document.name = i18n::translate(true, "Untitled motion").into_owned();
            for layer in &mut document.layers {
                layer.name = i18n::translate(true, &layer.name).into_owned();
            }
        }
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
            && self.brush_defaults.iter().all(|b| {
                [b.ink, b.liquify, b.marker, b.eraser]
                    .iter()
                    .all(|v| v.is_finite() && *v > 0.0)
            })
            && self.onion.opacity.is_finite()
            && (0.0..=1.0).contains(&self.onion.opacity)
            && self.onion.falloff.is_finite()
            && (0.0..=1.0).contains(&self.onion.falloff)
            && self
                .bindings
                .iter()
                .all(|b| eframe::egui::Key::from_name(&b.key).is_some())
            && self.bindings.iter().enumerate().all(|(i, b)| {
                self.bindings[i + 1..].iter().all(|other| {
                    b.action != other.action
                        && !(b.key == other.key
                            && b.control == other.control
                            && b.command == other.command
                            && b.alt == other.alt
                            && b.shift == other.shift)
                })
            })
            && default_bindings()
                .iter()
                .all(|required| self.bindings.iter().any(|b| b.action == required.action))
            && self.onion.previous <= 12
            && self.onion.next <= 12
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
                Ok(mut p) if p.valid() => {
                    p.bindings
                        .retain(|b| !matches!(b.action.as_str(), "Fit" | "Liquify"));
                    p.migrate_shortcuts();
                    p
                }
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
#[derive(Serialize, Deserialize)]
struct PreferenceBundle {
    format: String,
    version: u32,
    preferences: Preferences,
}
pub fn export_settings(path: &Path, preferences: &Preferences) -> Result<(), String> {
    if !preferences.valid() {
        return Err("Preferences are invalid".into());
    }
    let bundle = PreferenceBundle {
        format: "mori.preferences".into(),
        version: 1,
        preferences: preferences.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&bundle).map_err(|e| e.to_string())?;
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn import_settings(path: &Path) -> Result<Preferences, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let mut bundle: PreferenceBundle =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid settings file".to_owned())?;
    if bundle.format != "mori.preferences" || bundle.version != 1 {
        return Err("Unsupported settings format".into());
    }
    bundle
        .preferences
        .bindings
        .retain(|b| !matches!(b.action.as_str(), "Fit" | "Liquify"));
    bundle.preferences.migrate_shortcuts();
    if !bundle.preferences.valid() {
        return Err("Preferences are invalid".into());
    }
    Ok(bundle.preferences)
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

#[cfg(test)]
mod pause_tests {
    use super::*;
    #[test]
    fn default_pause_returns_to_origin_and_keep_playhead_snaps_current_time() {
        let timing = Timing::default();
        let preferences = Preferences::default();
        assert_eq!(
            preferences.pause_behavior.position(&timing, 0.1875, 1.26),
            0.1875
        );
        assert_eq!(
            PauseBehavior::KeepPlayhead.position(&timing, 0.1875, 1.26),
            1.25
        );
    }
    #[test]
    fn pause_preference_survives_restart_and_older_files_get_return_to_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("preferences.json");
        let (mut store, mut preferences, _) = PreferenceStore::at(path.clone());
        preferences.pause_behavior = PauseBehavior::KeepPlayhead;
        store.save_if_changed(&preferences).unwrap();
        let (_, loaded, _) = PreferenceStore::at(path);
        assert_eq!(loaded.pause_behavior, PauseBehavior::KeepPlayhead);
        let mut older = serde_json::to_value(loaded).unwrap();
        older.as_object_mut().unwrap().remove("pause_behavior");
        let migrated: Preferences = serde_json::from_value(older).unwrap();
        assert_eq!(migrated.pause_behavior, PauseBehavior::ReturnToStart);
    }
}

#[cfg(test)]
mod new_default_tests {
    use super::*;
    #[test]
    fn defaults_have_zero_minimum_three_onions_and_watermark_enabled() {
        let p = Preferences::default();
        assert_eq!(p.minimum_pressure, 0.0);
        assert_eq!(p.video_delay_ms, -64.0);
        assert_eq!((p.onion.previous, p.onion.next), (3, 3));
        assert!(p.onion.falloff < 1.0);
        assert!(p.new_document().settings.watermark);
        assert!(!p
            .bindings
            .iter()
            .any(|b| matches!(b.action.as_str(), "Fit" | "Liquify")));
    }
    #[test]
    fn marker_width_is_independent_and_watermark_defaults_are_local_to_new_projects() {
        let mut p = Preferences {
            default_watermark: false,
            ..Default::default()
        };
        p.brush_defaults[1].marker = 78.0;
        let mut doc = p.new_document();
        assert!(!doc.settings.watermark);
        doc.settings.watermark = true;
        assert!(!p.new_document().settings.watermark);
        assert_eq!(doc.layers[1].brush_width(Brush::Marker), 78.0);
        assert_eq!(doc.layers[1].brush_width(Brush::Ink), 42.0);
    }
}

#[cfg(test)]
mod settings_transfer_tests {
    use super::*;
    #[test]
    fn follow_and_eraser_defaults_migrate_and_persist_independently() {
        let mut legacy = serde_json::to_value(Preferences::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("timeline_follow");
        for defaults in legacy["brush_defaults"].as_array_mut().unwrap() {
            defaults.as_object_mut().unwrap().remove("eraser");
        }
        let mut prefs: Preferences = serde_json::from_value(legacy).unwrap();
        assert!(prefs.timeline_follow);
        assert_eq!(prefs.brush_defaults[0].eraser, 42.0);
        prefs.timeline_follow = false;
        prefs.brush_defaults[0].eraser = 96.0;
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _, _) = PreferenceStore::at(dir.path().join("prefs.json"));
        store.save_if_changed(&prefs).unwrap();
        let (_, prefs, warning) = PreferenceStore::at(store.path);
        assert!(warning.is_none());
        assert!(!prefs.timeline_follow);
        let mut layer = prefs.layer("Test", Blend::Base);
        assert_eq!(layer.brush_width(Brush::Eraser), 96.0);
        *layer.brush_width_mut(Brush::Eraser) = 128.0;
        assert_eq!(layer.brush_width(Brush::Ink), 6.0);
        let saved = serde_json::to_value(&layer).unwrap();
        let restored: Layer = serde_json::from_value(saved.clone()).unwrap();
        assert_eq!(restored.brush_width(Brush::Eraser), 128.0);
        let mut legacy = saved;
        legacy.as_object_mut().unwrap().remove("eraser_width");
        let restored: Layer = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.brush_width(Brush::Eraser), 42.0);
    }
    #[test]
    fn settings_export_import_roundtrips_all_preferences_and_not_project_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut preferences = Preferences {
            language: Language::Chinese,
            video_delay_ms: -77.0,
            minimum_pressure: 0.12,
            ..Default::default()
        };
        preferences.brush_defaults[0].marker = 87.0;
        preferences.onion.before = [10, 20, 30];
        preferences.pause_behavior = PauseBehavior::KeepPlayhead;
        export_settings(&path, &preferences).unwrap();
        let imported = import_settings(&path).unwrap();
        assert_eq!(
            serde_json::to_value(&preferences).unwrap(),
            serde_json::to_value(&imported).unwrap()
        );
        let bytes = fs::read_to_string(&path).unwrap();
        assert!(!bytes.contains("\"cels\"") && !bytes.contains("\"tempos\""));
        let mut project = Document::default();
        project.layers[0].width = 52.0;
        let before = serde_json::to_value(&project).unwrap();
        let _global = import_settings(&path).unwrap();
        assert_eq!(serde_json::to_value(project).unwrap(), before);
    }
    #[test]
    fn import_rejects_unknown_format_invalid_numbers_and_conflicting_bindings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let preferences = Preferences::default();
        export_settings(&path, &preferences).unwrap();
        let original: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        for update in ["format", "spacing", "bindings"] {
            let mut json = original.clone();
            match update {
                "format" => json["version"] = 999.into(),
                "spacing" => json["preferences"]["spacing"] = 0.into(),
                _ => {
                    json["preferences"]["bindings"][1] = json["preferences"]["bindings"][0].clone();
                }
            }
            fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
            assert!(import_settings(&path).is_err());
        }
        fs::write(&path, b"{}").unwrap();
        assert!(import_settings(&path).is_err());
    }
    #[test]
    fn old_shipped_undo_bindings_migrate_and_custom_shortcuts_stay_unchanged() {
        let mut p = Preferences::default();
        let undo = p.bindings.iter_mut().find(|b| b.action == "Undo").unwrap();
        undo.command = false;
        let redo = p.bindings.iter_mut().find(|b| b.action == "Redo").unwrap();
        redo.key = "Y".into();
        redo.command = false;
        redo.shift = false;
        p.migrate_shortcuts();
        assert!(
            p.bindings
                .iter()
                .find(|b| b.action == "Undo")
                .unwrap()
                .command
        );
        let redo = p.bindings.iter().find(|b| b.action == "Redo").unwrap();
        assert_eq!(redo.key, "Z");
        assert!(redo.command && redo.shift);
        let undo = p.bindings.iter_mut().find(|b| b.action == "Undo").unwrap();
        undo.key = "U".into();
        undo.alt = true;
        let customized = serde_json::to_value(&p).unwrap();
        p.migrate_shortcuts();
        assert_eq!(serde_json::to_value(&p).unwrap(), customized);
    }
}
