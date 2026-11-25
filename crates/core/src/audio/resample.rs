//! 任意采样率单声道 → 16kHz 单声道（rubato FFT 重采样）

use anyhow::{Result, anyhow};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

use super::TARGET_RATE;

pub struct ToTarget {
    inner: Option<Fft<f32>>,
    pending: Vec<f32>,
    out: Vec<f32>,
}

impl ToTarget {
    pub fn new(input_rate: u32) -> Result<Self> {
        let inner = if input_rate == TARGET_RATE {
            None
        } else {
            let rs = Fft::<f32>::new(input_rate as usize, TARGET_RATE as usize, 1024, 1, FixedSync::Input)
                .map_err(|e| anyhow!("创建重采样器失败（{input_rate}Hz）：{e}"))?;
            Some(rs)
        };
        Ok(Self { inner, pending: Vec::new(), out: Vec::new() })
    }

    /// 推入原始采样，返回已转换好的 16kHz 采样（可能为空，不足一块时先缓存）
    pub fn push(&mut self, input: &[f32]) -> Result<Vec<f32>> {
        let Some(rs) = self.inner.as_mut() else {
            return Ok(input.to_vec());
        };
        self.pending.extend_from_slice(input);
        let mut result = Vec::new();
        loop {
            let need = rs.input_frames_next();
            if self.pending.len() < need {
                break;
            }
            let out_max = rs.output_frames_max();
            self.out.resize(out_max, 0.0);
            let input = InterleavedSlice::new(&self.pending[..need], 1, need).map_err(|e| anyhow!("{e:?}"))?;
            let mut output = InterleavedSlice::new_mut(&mut self.out[..], 1, out_max).map_err(|e| anyhow!("{e:?}"))?;
            let (n_in, n_out) = rs
                .process_into_buffer(&input, &mut output, None)
                .map_err(|e| anyhow!("重采样失败：{e}"))?;
            result.extend_from_slice(&self.out[..n_out]);
            self.pending.drain(..n_in);
        }
        Ok(result)
    }
}
