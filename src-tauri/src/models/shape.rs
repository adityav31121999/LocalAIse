//! GGUF parsing: read the model facts the memory budget needs.
//!
//! Only the header is read — tensor data is never touched. The metadata keys
//! are architecture-prefixed, e.g. `qwen2.block_count`, so everything here
//! matches on the suffix after the last `.` in the key.

use std::io::Read;
use std::path::Path;

use crate::runtime::budget::{ModelShape, MoeShape};

const MAGIC: u32 = 0x46554747; // "GGUF" little-endian

/// How much of the metadata block to read.
///
/// The facts the UI needs sit at the front of the file, but the tokenizer
/// vocabulary that follows can run into megabytes. Reading a bounded prefix
/// keeps a folder scan fast even with a few dozen models on disk.
const META_BUDGET_BYTES: u64 = 512 * 1024;

/// Metadata the model list shows next to a file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelMeta {
    /// Architecture prefix from `general.architecture`, e.g. "llama".
    pub architecture: String,
    /// The model's own name from `general.name`.
    pub name: String,
    /// Parameter count when the file declares one.
    pub parameter_count: u64,
    /// True for a multimodal projector (a `clip.*` GGUF).
    pub is_projector: bool,
}

/// A model the budget can size honestly.
pub struct ShapeResult {
    pub shape: ModelShape,
    /// Architecture, name and parameter count from the same header.
    pub meta: ModelMeta,
    /// Filled when the file could not be read as a GGUF header.
    pub warning: Option<String>,
}

/// Counts the bytes read so the metadata loop can stop at its budget.
struct Metered<R> {
    inner: R,
    read: u64,
}

impl<R> Metered<R> {
    fn new(inner: R) -> Self {
        Self { inner, read: 0 }
    }
}

impl<R: Read> Read for Metered<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(buf)?;
        self.read += count as u64;
        Ok(count)
    }
}

fn read_u32(r: &mut impl Read) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf).map_err(io_string)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_u64(r: &mut impl Read) -> Result<u64, String> {
    let mut buf = [0u8; 8];
    r.read_exact(&mut buf).map_err(io_string)?;
    Ok(u64::from_le_bytes(buf))
}

fn read_string(r: &mut impl Read) -> Result<String, String> {
    let len = read_u64(r)?;
    if len > 1 << 20 {
        return Err("string too long for GGUF metadata".into());
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).map_err(io_string)?;
    String::from_utf8(buf).map_err(|_| "invalid utf8 in GGUF metadata".into())
}

fn io_string(error: std::io::Error) -> String {
    error.to_string()
}

/// Reads the GGUF metadata needed for memory estimates.
pub fn read_shape(path: &Path) -> ShapeResult {
    let file_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

    let mut shape = ModelShape {
        file_bytes,
        n_layers: 0,
        n_kv_heads: 0,
        head_dim: 0,
        max_context: 0,
        moe: None,
    };
    let mut meta = ModelMeta::default();

    let warning = std::fs::File::open(path)
        .map_err(|_| "model file could not be opened".to_string())
        .and_then(|file| {
            let mut reader = Metered::new(std::io::BufReader::new(file));
            read_shape_into(&mut reader, &mut shape, &mut meta)
        })
        .err();

    ShapeResult { shape, meta, warning }
}

/// Reads only the metadata the model list needs — architecture, name and
/// parameter count. The tensor list is never touched, so this is cheap
/// enough to run over every GGUF in a folder.
pub fn read_meta(path: &Path) -> ModelMeta {
    let Ok(file) = std::fs::File::open(path) else {
        return ModelMeta::default();
    };

    let mut reader = Metered::new(std::io::BufReader::new(file));
    let Ok((_, metadata)) = read_header(&mut reader) else {
        return ModelMeta::default();
    };

    let mut meta = ModelMeta::default();
    fill_meta(&metadata, &mut meta);
    meta
}

fn read_shape_into<R: Read>(
    reader: &mut Metered<R>,
    shape: &mut ModelShape,
    meta: &mut ModelMeta,
) -> Result<(), String> {
    let (tensor_count, metadata) = read_header(reader)?;

    fill_meta(&metadata, meta);

    // Layers and attention shape. Values are usually u32, occasionally f32.
    shape.n_layers = metadata.num("block_count");
    let n_heads = metadata.num("head_count");
    let n_kv_heads = metadata.num("head_count_kv");
    shape.n_kv_heads = if n_kv_heads > 0 {
        n_kv_heads
    } else {
        n_heads.max(1)
    };
    shape.head_dim = metadata.num("key_length");

    if shape.head_dim == 0 && n_heads > 0 {
        let embedding_length = metadata.num("embedding_length");
        if embedding_length > 0 {
            shape.head_dim = embedding_length / n_heads;
        }
    }

    // The context length the model was trained for; 0 when the file does
    // not declare it.
    shape.max_context = metadata.num("context_length");

    // Mixture-of-experts facts. expert_count 0 means a dense model; the
    // tensor list is then not parsed at all.
    let n_experts = metadata.num("expert_count");
    if n_experts == 0 {
        shape.moe = None;
        return Ok(());
    }
    if tensor_count == 0 {
        shape.moe = Some(MoeShape {
            n_experts,
            n_experts_used: metadata.num("expert_used_count"),
            expert_bytes_total: 0,
        });
        return Ok(());
    }

    let mut expert_bytes = 0f64;
    for _ in 0..tensor_count {
        let name = read_string(reader)?;
        let n_dims = read_u32(reader)?;
        let mut elements = 1u64;
        for _ in 0..n_dims {
            elements = elements.saturating_mul(read_u64(reader)?);
        }
        let ggml_type = read_u32(reader)?;
        let _offset = read_u64(reader)?;

        if name.contains("exps") {
            expert_bytes += elements as f64 * ggml_type_scale(ggml_type);
        }
    }

    shape.moe = Some(MoeShape {
        n_experts,
        n_experts_used: metadata.num("expert_used_count"),
        expert_bytes_total: expert_bytes as u64,
    });

    Ok(())
}

/// Reads the magic, the counts and the key/value metadata block.
///
/// The metadata loop stops at [`META_BUDGET_BYTES`]; anything past that is
/// tokenizer detail the app never looks at.
fn read_header<R: Read>(reader: &mut Metered<R>) -> Result<(u64, Metadata), String> {
    if read_u32(reader)? != MAGIC {
        return Err("not a GGUF file (bad magic)".into());
    }
    let _version = read_u32(reader)?;
    let tensor_count = read_u64(reader)?;
    let kv_count = read_u64(reader)?;

    let mut metadata = Metadata::default();
    for _ in 0..kv_count {
        if reader.read >= META_BUDGET_BYTES {
            break;
        }
        let key = read_string(reader)?;
        let value_type = read_u32(reader)?;
        read_value(reader, value_type, &key, &mut metadata)?;
    }

    Ok((tensor_count, metadata))
}

/// Copies the display facts out of the raw metadata.
fn fill_meta(metadata: &Metadata, meta: &mut ModelMeta) {
    meta.architecture = metadata
        .text("general.architecture")
        .or_else(|| metadata.text("architecture"))
        .unwrap_or_default()
        .to_string();
    meta.name = metadata
        .text("general.name")
        .or_else(|| metadata.text("name"))
        .unwrap_or_default()
        .to_string();
    meta.parameter_count = metadata.num64("parameter_count").unwrap_or(0);
    // A multimodal projector is a CLIP vision tower, not a language model.
    meta.is_projector = meta.architecture.starts_with("clip")
        || meta.architecture.starts_with("mamba-clip")
        || meta.architecture.starts_with("mmproj");
}

#[derive(Default)]
struct Metadata {
    numbers: Vec<(String, f64)>,
    strings: Vec<(String, String)>,
}

impl Metadata {
    fn put(&mut self, key: &str, value: f64) {
        self.numbers.push((key.to_string(), value));
    }

    fn put_str(&mut self, key: &str, value: String) {
        self.strings.push((key.to_string(), value));
    }

    /// Value of a metadata key matched by suffix (after the arch prefix).
    fn num(&self, suffix: &str) -> u32 {
        self.numbers
            .iter()
            .filter(|(key, _)| key.rsplit('.').next() == Some(suffix))
            .map(|(_, value)| *value)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0) as u32
    }

    /// 64-bit variant, for counts that overflow a u32.
    fn num64(&self, suffix: &str) -> Option<u64> {
        self.numbers
            .iter()
            .filter(|(key, _)| key.rsplit('.').next() == Some(suffix))
            .map(|(_, value)| *value)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .filter(|value| *value > 0.0)
            .map(|value| value as u64)
    }

    /// A string value, matched by the full key first and by suffix after.
    fn text(&self, key: &str) -> Option<&str> {
        self.strings
            .iter()
            .find(|(stored, _)| stored == key)
            .or_else(|| {
                self.strings
                    .iter()
                    .find(|(stored, _)| stored.rsplit('.').next() == Some(key))
            })
            .map(|(_, value)| value.as_str())
    }
}

fn read_value(
    reader: &mut impl Read,
    value_type: u32,
    key: &str,
    metadata: &mut Metadata,
) -> Result<(), String> {
    match value_type {
        // u8 / bool
        0 | 7 => {
            let mut buf = [0u8; 1];
            reader.read_exact(&mut buf).map_err(io_string)?;
        }
        // i8 / u16 / i16
        1..=3 => {
            let mut buf = [0u8; 2];
            reader.read_exact(&mut buf).map_err(io_string)?;
        }
        // u32 / i32 / f32 — the types the shape keys use.
        4..=6 => {
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf).map_err(io_string)?;
            let value = match value_type {
                4 => u32::from_le_bytes(buf) as f64,
                5 => i32::from_le_bytes(buf) as f64,
                _ => f32::from_le_bytes(buf) as f64,
            };
            metadata.put(key, value);
        }
        // u64 / i64 / f64
        10..=12 => {
            let mut buf = [0u8; 8];
            reader.read_exact(&mut buf).map_err(io_string)?;
            let value = match value_type {
                10 => u64::from_le_bytes(buf) as f64,
                11 => i64::from_le_bytes(buf) as f64,
                _ => f64::from_le_bytes(buf),
            };
            metadata.put(key, value);
        }
        8 => {
            let value = read_string(reader)?;
            metadata.put_str(key, value);
        }
        // Array: element type, count, then plain elements. GGUF forbids
        // nested arrays, so the recursion is exactly one level deep.
        9 => {
            let elem_type = read_u32(reader)?;
            let count = read_u64(reader)?;
            for _ in 0..count {
                read_value(reader, elem_type, key, metadata)?;
            }
        }
        other => return Err(format!("unsupported GGUF value type: {other}")),
    }
    Ok(())
}

/// Approximate bytes per element for the GGML tensor types GGUF records,
/// including per-block scale overhead. Rarer types fall back to neighbor
/// sizes, which is fine for expert tensors: in practice they are q8_0, f16 or
/// q4_k scale blocks.
fn ggml_type_scale(type_id: u32) -> f64 {
    match type_id {
        0 => 4.0,         // F32
        28 => 8.0,        // F64
        1 | 30 => 2.0,    // F16 / BF16
        2 => 18.0 / 32.0, // Q4_0
        3 => 20.0 / 32.0, // Q4_1
        6 => 22.0 / 32.0, // Q5_0
        7 => 24.0 / 32.0, // Q5_1
        8 => 34.0 / 32.0, // Q8_0
        9 => 35.0 / 32.0, // Q8_1
        10 => 84.0 / 512.0,  // Q2_K
        11 => 110.0 / 512.0, // Q3_K
        12 => 144.0 / 512.0, // Q4_K
        13 => 176.0 / 512.0, // Q5_K
        14 => 210.0 / 512.0, // Q6_K
        15 => 4.0,           // Q8_K
        _ => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_files_without_the_gguf_magic() {
        let path = std::env::temp_dir().join(format!("LocalAIse-bad-{}.bin", std::process::id()));
        std::fs::write(&path, b"definitely not a gguf file").unwrap();
        let expected_bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let result = read_shape(&path);
        assert_eq!(result.shape.n_layers, 0);
        assert_eq!(result.shape.file_bytes, expected_bytes);
        assert!(result.warning.is_some());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_reports_a_zeroed_shape() {
        let result = read_shape(Path::new("this file does not exist.gguf"));
        assert_eq!(result.shape.n_layers, 0);
        assert!(result.warning.is_some());
    }

    /// Writes a tiny GGUF header with the given key/value pairs so the
    /// metadata reader can be exercised without a real model file.
    fn write_gguf(path: &Path, strings: &[(&str, &str)], numbers: &[(&str, u64)]) {
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(&MAGIC.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes()); // version
        bytes.extend_from_slice(&0u64.to_le_bytes()); // tensor count
        bytes.extend_from_slice(&((strings.len() + numbers.len()) as u64).to_le_bytes());

        for (key, value) in strings {
            bytes.extend_from_slice(&(key.len() as u64).to_le_bytes());
            bytes.extend_from_slice(key.as_bytes());
            bytes.extend_from_slice(&8u32.to_le_bytes()); // string
            bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        for (key, value) in numbers {
            bytes.extend_from_slice(&(key.len() as u64).to_le_bytes());
            bytes.extend_from_slice(key.as_bytes());
            bytes.extend_from_slice(&10u32.to_le_bytes()); // u64
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn reads_architecture_and_parameter_count() {
        let path = std::env::temp_dir().join(format!("LocalAIse-meta-{}.gguf", std::process::id()));
        write_gguf(
            &path,
            &[("general.architecture", "qwen3"), ("general.name", "Qwen 3 8B")],
            &[("qwen3.parameter_count", 8_000_000_000)],
        );

        let meta = read_meta(&path);
        assert_eq!(meta.architecture, "qwen3");
        assert_eq!(meta.name, "Qwen 3 8B");
        assert_eq!(meta.parameter_count, 8_000_000_000);
        assert!(!meta.is_projector);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_clip_architecture_is_a_projector() {
        let path = std::env::temp_dir().join(format!("LocalAIse-clip-{}.gguf", std::process::id()));
        write_gguf(&path, &[("general.architecture", "clip")], &[]);

        let meta = read_meta(&path);
        assert!(meta.is_projector);

        let _ = std::fs::remove_file(&path);
    }
}
