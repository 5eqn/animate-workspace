use crate::model::Document;
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

pub fn save(path: &Path, document: &Document) -> Result<(), String> {
    document.validate()?;
    let parent = path.parent().unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    {
        let mut archive = ZipWriter::new(&mut temporary);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        archive
            .start_file("document.json", options)
            .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut archive, document).map_err(|e| e.to_string())?;
        if let Some(audio) = &document.audio {
            archive
                .start_file("audio.source", options)
                .map_err(|e| e.to_string())?;
            archive.write_all(&audio.bytes).map_err(|e| e.to_string())?;
        }
        archive.finish().map_err(|e| e.to_string())?;
    }
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
pub fn load(path: &Path) -> Result<Document, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut saved: serde_json::Value = serde_json::from_reader(
        archive
            .by_name("document.json")
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // The movement blend was removed. Do not reinterpret its effect strokes as
    // drawing ink; discard those layers when opening an older project.
    if let Some(layers) = saved.get_mut("layers").and_then(|v| v.as_array_mut()) {
        layers.retain(|layer| layer.get("blend").and_then(|v| v.as_str()) != Some("Move"));
    }
    let mut document: Document = serde_json::from_value(saved).map_err(|e| e.to_string())?;
    if document.layers.is_empty() {
        document.layers.push(crate::model::Layer::new(
            "Paper",
            crate::model::Blend::Xor,
            6.0,
        ));
    }
    if let Some(audio) = &mut document.audio {
        let mut bytes = Vec::new();
        archive
            .by_name("audio.source")
            .map_err(|e| e.to_string())?
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        audio.bytes = Arc::new(bytes);
    }
    document.validate()?;
    for layer in &mut document.layers {
        if layer.blend == crate::model::Blend::Base {
            layer.blend = crate::model::Blend::Xor;
        }
    }
    document.snap_exposures();
    for layer in &mut document.layers {
        for cel in layer.cels.values_mut() {
            for stroke in &mut Arc::make_mut(cel).strokes {
                Arc::make_mut(stroke).id = crate::model::next_revision();
            }
        }
    }
    Ok(document)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_shared_translation_becomes_ordinary_vector_coordinates() {
        let mut cel = crate::model::Cel::default();
        let point = crate::model::Point {
            x: 80.0,
            y: 120.0,
            pressure: 0.6,
            angle: 0.3,
        };
        cel.strokes
            .push(Arc::new(crate::model::Stroke::new(24.0, point)));
        let mut saved = serde_json::to_value(&cel).unwrap();
        saved["translation"] = serde_json::json!([70.0, -30.0]);
        let loaded: crate::model::Cel = serde_json::from_value(saved).unwrap();
        assert_eq!(
            loaded.strokes[0].points[0],
            crate::model::Point {
                x: 150.0,
                y: 90.0,
                ..point
            }
        );
        assert_eq!(loaded.strokes[0].width, 24.0);
        assert!(!serde_json::to_value(loaded)
            .unwrap()
            .to_string()
            .contains("translation"));
    }
    #[test]
    fn opening_legacy_projects_removes_movement_layers_and_keeps_other_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.mori");
        let doc = Document::default();
        let mut saved = serde_json::to_value(&doc).unwrap();
        let mut movement = saved["layers"][0].clone();
        movement["blend"] = "Move".into();
        movement["name"] = "Old movement".into();
        saved["layers"]
            .as_array_mut()
            .unwrap()
            .insert(1, movement.clone());
        let write = |saved: &serde_json::Value| {
            let mut zip = ZipWriter::new(File::create(&path).unwrap());
            zip.start_file("document.json", SimpleFileOptions::default())
                .unwrap();
            serde_json::to_writer(&mut zip, saved).unwrap();
            zip.finish().unwrap();
        };
        write(&saved);
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.layers.len(), 3);
        assert_eq!(
            loaded
                .layers
                .iter()
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>(),
            ["Paper", "Ink", "Atmosphere"]
        );
        saved["layers"] = serde_json::json!([movement]);
        write(&saved);
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.layers.len(), 1);
        assert_eq!(loaded.layers[0].blend, crate::model::Blend::Xor);
    }
    #[test]
    fn project_round_trip_embeds_audio_and_vectors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.mori");
        let mut doc = Document {
            audio: Some(crate::model::AudioAsset {
                name: "song.wav".into(),
                bytes: Arc::new(vec![1, 2, 3, 4]),
            }),
            ..Default::default()
        };
        let point = crate::model::Point {
            x: 120.0,
            y: 80.0,
            pressure: 0.6,
            angle: 0.3,
        };
        doc.layers[0]
            .begin_edit(0, false)
            .strokes
            .push(Arc::new(crate::model::Stroke::new(24.0, point)));
        doc.layers[0].begin_edit(0, true).translate([160.0, -40.0]);
        save(&path, &doc).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(
            loaded.layers[0].cels[&0].strokes[0].points[0],
            crate::model::Point {
                x: point.x + 160.0,
                y: point.y - 40.0,
                ..point
            }
        );
        assert_eq!(loaded.audio.unwrap().bytes.as_slice(), [1, 2, 3, 4]);
        doc.name = "Replaced".into();
        save(&path, &doc).unwrap();
        assert_eq!(load(&path).unwrap().name, "Replaced");
    }
}
