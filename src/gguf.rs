//! Minimal GGUF metadata parser.
//! Goal: Automatically understand GGUF files without relying only on filenames.
//! This is part of the 10/10 professional-grade model integration effort.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct GgufMetadata {
    pub architecture: Option<String>,
    pub name: Option<String>,
    pub context_length: Option<u32>,
    pub parameter_count: Option<u64>,
    pub quantization: Option<String>,
    pub is_vision: bool,
}

/// Reads basic metadata from a GGUF file header.
/// This is a lightweight parser focused on the KV metadata section.
pub fn read_gguf_metadata(path: &Path) -> Result<GgufMetadata, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open GGUF: {}", e))?;
    let mut reader = BufReader::new(file);
    read_gguf_metadata_from_reader(&mut reader)
}

/// Fuzz-friendly entry: parse GGUF metadata directly from bytes (no file).
/// Used by `fuzz/fuzz_targets/gguf_parse.rs` and tests.
pub fn parse_gguf_bytes(data: &[u8]) -> Result<GgufMetadata, String> {
    let cursor = std::io::Cursor::new(data);
    let mut reader = BufReader::new(cursor);
    read_gguf_metadata_from_reader(&mut reader)
}

fn read_gguf_metadata_from_reader<R: Read + Seek>(reader: &mut BufReader<R>) -> Result<GgufMetadata, String> {
    // GGUF Magic: "GGUF" (4 bytes, little endian)
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic).map_err(|e| format!("Failed to read magic: {}", e))?;
    if &magic != b"GGUF" {
        return Err("Not a valid GGUF file (wrong magic)".to_string());
    }

    // Version (u32)
    let mut version_bytes = [0u8; 4];
    reader.read_exact(&mut version_bytes).map_err(|e| format!("Failed to read version: {}", e))?;
    let version = u32::from_le_bytes(version_bytes);

    if version < 2 {
        return Err(format!("GGUF version {} is too old and unsupported", version));
    }

    // Tensor count + KV count
    let mut tensor_count_bytes = [0u8; 8];
    let mut kv_count_bytes = [0u8; 8];
    reader.read_exact(&mut tensor_count_bytes).map_err(|e| format!("Failed to read tensor count: {}", e))?;
    reader.read_exact(&mut kv_count_bytes).map_err(|e| format!("Failed to read KV count: {}", e))?;

    let kv_count = u64::from_le_bytes(kv_count_bytes);

    let mut meta = GgufMetadata::default();

    for _ in 0..kv_count {
        match read_kv_pair_generic(reader) {
            Ok((key, value)) => {
                apply_kv(&mut meta, &key, value);
            }
            Err(_) => {
                // Skip unknown or malformed KV entries gracefully
                continue;
            }
        }
    }

    // Fallback vision detection using architecture if metadata didn't have it
    if !meta.is_vision {
        if let Some(arch) = &meta.architecture {
            let arch_lower = arch.to_lowercase();
            if arch_lower.contains("clip") || arch_lower.contains("vision") || arch_lower.contains("vl") {
                meta.is_vision = true;
            }
        }
    }

    Ok(meta)
}

/// Reads one KV pair. Simplified implementation — generic over Read+Seek for fuzzing.
fn read_kv_pair<R: Read + Seek>(reader: &mut BufReader<R>) -> Result<(String, GgufValue), String> {
    read_kv_pair_generic(reader)
}
fn read_kv_pair_generic<R: Read + Seek>(reader: &mut BufReader<R>) -> Result<(String, GgufValue), String> {
    // Read key length (u64)
    let mut key_len_bytes = [0u8; 8];
    reader.read_exact(&mut key_len_bytes).map_err(|e| e.to_string())?;
    let key_len = u64::from_le_bytes(key_len_bytes) as usize;

    let mut key_bytes = vec![0u8; key_len];
    reader.read_exact(&mut key_bytes).map_err(|e| e.to_string())?;
    let key = String::from_utf8(key_bytes).map_err(|e| e.to_string())?;

    // Read value type (u32)
    let mut type_bytes = [0u8; 4];
    reader.read_exact(&mut type_bytes).map_err(|e| e.to_string())?;
    let value_type = u32::from_le_bytes(type_bytes);

    let value = match value_type {
        0 => { // UINT8
            let mut b = [0u8; 1];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Uint8(b[0])
        }
        1 => { // INT8
            let mut b = [0u8; 1];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Int8(b[0] as i8)
        }
        2 => { // UINT16
            let mut b = [0u8; 2];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Uint16(u16::from_le_bytes(b))
        }
        3 => { // INT16
            let mut b = [0u8; 2];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Int16(i16::from_le_bytes(b))
        }
        4 => { // UINT32
            let mut b = [0u8; 4];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Uint32(u32::from_le_bytes(b))
        }
        5 => { // INT32
            let mut b = [0u8; 4];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Int32(i32::from_le_bytes(b))
        }
        6 => { // FLOAT32
            let mut b = [0u8; 4];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Float32(f32::from_le_bytes(b))
        }
        7 => { // BOOL
            let mut b = [0u8; 1];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Bool(b[0] != 0)
        }
        8 => { // STRING
            let mut len_bytes = [0u8; 8];
            reader.read_exact(&mut len_bytes).map_err(|e| e.to_string())?;
            let len = u64::from_le_bytes(len_bytes) as usize;
            let mut data = vec![0u8; len];
            reader.read_exact(&mut data).map_err(|e| e.to_string())?;
            let s = String::from_utf8_lossy(&data).to_string();
            GgufValue::String(s)
        }
        9 => { // ARRAY - skip properly by element type
            let mut elem_type_bytes = [0u8; 4];
            reader.read_exact(&mut elem_type_bytes).map_err(|e| e.to_string())?;
            let elem_type = u32::from_le_bytes(elem_type_bytes);
            let mut len_bytes = [0u8; 8];
            reader.read_exact(&mut len_bytes).map_err(|e| e.to_string())?;
            let len = u64::from_le_bytes(len_bytes);
            for _ in 0..len {
                match elem_type {
                    0 | 1 | 7 => { // UINT8, INT8, BOOL = 1 byte
                        reader.seek(SeekFrom::Current(1)).map_err(|e| e.to_string())?;
                    }
                    2 | 3 => { // UINT16, INT16 = 2 bytes
                        reader.seek(SeekFrom::Current(2)).map_err(|e| e.to_string())?;
                    }
                    4 | 5 | 6 => { // UINT32, INT32, FLOAT32 = 4 bytes
                        reader.seek(SeekFrom::Current(4)).map_err(|e| e.to_string())?;
                    }
                    8 => { // STRING = 8 byte len + data
                        let mut slen_bytes = [0u8; 8];
                        reader.read_exact(&mut slen_bytes).map_err(|e| e.to_string())?;
                        let slen = u64::from_le_bytes(slen_bytes) as i64;
                        reader.seek(SeekFrom::Current(slen)).map_err(|e| e.to_string())?;
                    }
                    10 | 11 | 12 => { // UINT64, INT64, FLOAT64 = 8 bytes
                        reader.seek(SeekFrom::Current(8)).map_err(|e| e.to_string())?;
                    }
                    _ => {
                        // Unknown element type: fall back to 8-byte skip to avoid infinite loop; log and break
                        log::warn!("Unknown GGUF array element type {} during skip, using 8-byte fallback", elem_type);
                        reader.seek(SeekFrom::Current(8)).map_err(|e| e.to_string())?;
                    }
                }
            }
            GgufValue::Array
        }
        10 => { // UINT64
            let mut b = [0u8; 8];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Uint64(u64::from_le_bytes(b))
        }
        11 => { // INT64
            let mut b = [0u8; 8];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Int64(i64::from_le_bytes(b))
        }
        12 => { // FLOAT64
            let mut b = [0u8; 8];
            reader.read_exact(&mut b).map_err(|e| e.to_string())?;
            GgufValue::Float64(f64::from_le_bytes(b))
        }
        _ => {
            return Err(format!("Unsupported GGUF value type: {}", value_type));
        }
    };

    Ok((key, value))
}

#[derive(Debug)]
enum GgufValue {
    Uint8(u8),
    Int8(i8),
    Uint16(u16),
    Int16(i16),
    Uint32(u32),
    Int32(i32),
    Float32(f32),
    Bool(bool),
    String(String),
    Array,
    Uint64(u64),
    Int64(i64),
    Float64(f64),
}

fn apply_kv(meta: &mut GgufMetadata, key: &str, value: GgufValue) {
    match key {
        "general.architecture" => {
            if let GgufValue::String(s) = value {
                meta.architecture = Some(s);
            }
        }
        "general.name" => {
            if let GgufValue::String(s) = value {
                meta.name = Some(s);
            }
        }
        "general.parameter_count" => {
            if let GgufValue::Uint64(n) = value {
                meta.parameter_count = Some(n);
            }
        }
        "llama.context_length" | "qwen2.context_length" | "qwen3.context_length" => {
            if let GgufValue::Uint32(n) = value {
                meta.context_length = Some(n);
            } else if let GgufValue::Uint64(n) = value {
                meta.context_length = Some(n as u32);
            }
        }
        // Add more important keys as needed
        _ => {}
    }

    // Rough vision detection from known keys
    if key.contains("vision") || key.contains("clip") {
        meta.is_vision = true;
    }
}
