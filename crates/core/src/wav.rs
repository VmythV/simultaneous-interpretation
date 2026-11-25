//! WAV 读写（16kHz 单声道 16-bit）

use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::audio::TARGET_RATE;

fn spec() -> hound::WavSpec {
    hound::WavSpec { channels: 1, sample_rate: TARGET_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int }
}

pub fn f32_to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// 编码成内存中的 WAV 文件（用于上传识别）
pub fn encode(samples: &[f32]) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::with_capacity(44 + samples.len() * 2));
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec())?;
        for &s in samples {
            w.write_sample(f32_to_i16(s))?;
        }
        w.finalize()?;
    }
    Ok(cursor.into_inner())
}

pub fn write(path: &Path, samples: &[f32]) -> Result<()> {
    let mut w = hound::WavWriter::create(path, spec()).with_context(|| format!("无法写入 {}", path.display()))?;
    for &s in samples {
        w.write_sample(f32_to_i16(s))?;
    }
    w.finalize()?;
    Ok(())
}

/// 读取 16kHz 单声道 WAV（其他格式请先转换）
pub fn read(path: &Path) -> Result<Vec<f32>> {
    let r = hound::WavReader::open(path).with_context(|| format!("无法打开 {}", path.display()))?;
    decode(r)
}

/// 从内存读取 16kHz 单声道 WAV
pub fn read_bytes(bytes: &[u8]) -> Result<Vec<f32>> {
    decode(hound::WavReader::new(Cursor::new(bytes))?)
}

fn decode<R: std::io::Read>(mut r: hound::WavReader<R>) -> Result<Vec<f32>> {
    let spec = r.spec();
    if spec.channels != 1 || spec.sample_rate != TARGET_RATE {
        bail!("需要 16kHz 单声道 WAV，实际是 {}Hz / {} 声道", spec.sample_rate, spec.channels);
    }
    let samples = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().map(|s| s.map(|v| v as f32 / max)).collect::<Result<Vec<_>, _>>()?
        }
        hound::SampleFormat::Float => r.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
    };
    Ok(samples)
}
