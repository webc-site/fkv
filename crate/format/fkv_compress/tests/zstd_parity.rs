//! mzstd 与 zstd 1.5.7（C 实现，经 `zstd` crate）的逐字节对拍回归。
//!
//! 覆盖生产使用的两条路径：
//! - raw：Rust `compress_raw`（复用线程内 CCtx）↔ C `ZSTD_compressCCtx`；
//! - 字典：Rust `DictCompression::compress`（mzstd `PreparedDictionary`）↔
//!   C `ZSTD_createCDict` + `ZSTD_compress_usingCDict`（C++ `DictCompression::Compress`）。
//!
//! 版本前置：`zstd` / `zstd-sys` 在 workspace 中精确 pin 到 0.14.3 / 2.1.2
//! （内嵌 zstd 1.5.7）；本文件的首个用例在运行时再断言一次，防止 `pkg-config`
//! 等环境变量改链到别的系统库而测试仍"通过"。

use std::iter::repeat_n;

use fkv_compress::{DictCompression, compress_raw};
use zstd::{
  dict::from_continuous,
  zstd_safe::{CCtx, CDict, compress_bound, version_number},
};
use zstd_sys::ZSTD_versionNumber;

/// 确定性伪随机（splitmix64），避免对拍依赖外部随机源。
fn splitmix64(state: &mut u64) -> u64 {
  *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
  let mut z = *state;
  z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
  z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
  z ^ (z >> 31)
}

/// 三类语料：结构化文本（熵表 + 长匹配）、伪随机（字面量块）、文本/随机/重复混合。
fn corpus(kind: usize, len: usize) -> Vec<u8> {
  let mut state = 0x1234_5678_9ABC_DEF0 ^ (len as u64) << 17 ^ kind as u64;
  let mut out = Vec::with_capacity(len);
  match kind {
    0 => {
      while out.len() < len {
        out.extend_from_slice(
          format!(
            "user:{}\tcity:Oslo\tactive:{}\tpayload={}\n",
            splitmix64(&mut state) % 1000,
            splitmix64(&mut state) & 1,
            splitmix64(&mut state) % 100_000
          )
          .as_bytes(),
        );
      }
    }
    1 => {
      while out.len() < len {
        out.extend_from_slice(&splitmix64(&mut state).to_le_bytes());
      }
    }
    _ => {
      while out.len() < len {
        match splitmix64(&mut state) % 4 {
          0 => out.extend_from_slice(b"the quick brown fox jumps over the lazy dog\n"),
          1 => out.extend_from_slice(&splitmix64(&mut state).to_le_bytes()),
          2 => out.extend(repeat_n(0x5A, 17)),
          _ => out.extend_from_slice(b"{\"k\":1,\"v\":\"northwind\"}\n"),
        }
      }
    }
  }
  out.truncate(len);
  out
}

/// C 侧 raw 一帧；复用同一个 CCtx，逐次压缩等价 C++ `CompressRaw` 的
/// `ZSTD_compressCCtx`（每次调用都会 reset 并应用 level）。
fn c_raw(input: &[u8], level: i32, cctx: &mut CCtx<'static>) -> Vec<u8> {
  let mut out = Vec::with_capacity(compress_bound(input.len()));
  let written = cctx.compress(&mut out, input, level).unwrap();
  out.truncate(written);
  out
}

/// 从 `varint32 | frame` 里剥出帧体（对拍只比较帧，不比较两实现各自的前缀封装）。
fn frame_body(framed: &[u8]) -> &[u8] {
  let (_, off) = fkv_codec::get_varint32(framed).unwrap();
  &framed[off..]
}

fn assert_same(what: &str, level: i32, len: usize, ours: &[u8], reference: &[u8]) {
  if ours != reference {
    let pos = ours
      .iter()
      .zip(reference)
      .position(|(a, b)| a != b)
      .unwrap_or_else(|| ours.len().min(reference.len()));
    panic!(
      "{what} 字节不一致: level={level} len={len} 首个差异 offset={pos} \
       ours_len={} ref_len={}\n ours={:02X?}\n ref ={:02X?}",
      ours.len(),
      reference.len(),
      &ours[pos.saturating_sub(8)..(pos + 24).min(ours.len())],
      &reference[pos.saturating_sub(8)..(pos + 24).min(reference.len())],
    );
  }
}

/// C 参考实现必须是 zstd 1.5.7（`zstd-sys 2.1.2+zstd.1.5.7`，workspace 精确 pin）。
#[test]
fn c_reference_is_pinned_zstd_1_5_7() {
  // SAFETY: 无参数、无副作用，仅读取链接进二进制的 zstd 版本号。
  let linked = unsafe { ZSTD_versionNumber() };
  assert_eq!(linked, 10_507);
  assert_eq!(version_number(), 10_507);
}

/// raw 路径：同输入同 level 的压缩帧逐字节一致。
#[test]
fn raw_frames_are_byte_identical_to_zstd() {
  let cctx = &mut CCtx::create();
  // 覆盖帧头字段宽度边界（255/256、65535/65536）与 zstd 128 KiB 块边界。
  let lens = [
    1, 3, 16, 63, 100, 127, 128, 129, 255, 256, 1000, 4096, 16_384, 65_535, 65_536, 65_537,
    131_071, 131_072, 131_073, 262_144,
  ];
  let levels = [-5, 1, 3, 7];
  let mut checked = 0usize;
  for kind in 0..3 {
    for &len in &lens {
      let input = corpus(kind, len);
      for &level in &levels {
        let Some(ours) = compress_raw(&input, level).unwrap() else {
          continue;
        };
        let reference = c_raw(&input, level, cctx);
        assert_same("raw", level, len, frame_body(&ours), &reference);
        checked += 1;
      }
      // 最高级别只跑中小输入：debug 构建下大输入过慢，且参数选择差异集中在
      // opt 策略/窗口，长度维度的覆盖已由上面的 level 组完成。
      if len <= 4096 {
        let Some(ours) = compress_raw(&input, 19).unwrap() else {
          continue;
        };
        let reference = c_raw(&input, 19, cctx);
        assert_same("raw", 19, len, frame_body(&ours), &reference);
        checked += 1;
      }
    }
  }
  assert!(checked >= 100, "对拍样本退化: {checked}");
  eprintln!("raw 对拍用例: {checked}");
}

/// 字典路径：同一字典（生产用 `ZDICT_trainFromBuffer` 产出）下，
/// mzstd 的 `PreparedDictionary` 与 C 的 CDict 压缩帧逐字节一致。
#[test]
fn dictionary_frames_are_byte_identical_to_zstd() {
  let mut sample_data = Vec::new();
  let mut sizes = Vec::new();
  for i in 0..200u32 {
    let sample = corpus(0, 1200 + (i as usize % 7) * 64);
    sample_data.extend_from_slice(&sample);
    sizes.push(sample.len());
  }
  let dict = from_continuous(&sample_data, &sizes, 16 * 1024).unwrap();

  let lens = [100, 127, 128, 256, 1000, 4096, 65_536];
  let levels = [1, 3, 19];
  let mut checked = 0usize;
  for &level in &levels {
    let cdict = CDict::create(&dict, level);
    let cctx = &mut CCtx::create();
    let mut ours = DictCompression::new(level);
    ours.load_dictionary(dict.clone()).unwrap();
    for kind in 0..3 {
      for &len in &lens {
        let input = corpus(kind, len);
        let Some(compressed) = ours.compress(&input).unwrap() else {
          continue;
        };
        let mut out = Vec::with_capacity(compress_bound(input.len()));
        let written = cctx.compress_using_cdict(&mut out, &input, &cdict).unwrap();
        out.truncate(written);
        assert_same("dictionary", level, len, frame_body(&compressed), &out);
        checked += 1;
      }
    }
  }
  // 随机语料压缩后可能不小于原值（生产路径返回 None，不写盘），故只要求
  // 有足够的有效对拍样本，防止矩阵退化后测试静默通过。
  assert!(checked >= 30, "对拍样本退化: {checked}");
  eprintln!("dictionary 对拍用例: {checked}");
}
