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
    let mut document: Document = serde_json::from_reader(
        archive
            .by_name("document.json")
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
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
        save(&path, &doc).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.audio.unwrap().bytes.as_slice(), [1, 2, 3, 4]);
        doc.name = "Replaced".into();
        save(&path, &doc).unwrap();
        assert_eq!(load(&path).unwrap().name, "Replaced");
    }
}
