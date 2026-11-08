//! restart 二分 seek / seek_floor：与线性参考实现逐点对比 + 边界。

use fkv_page::{
  CompressionType, DataPageBuilder, DataPageIter, EntrySpec, IndexPageBuilder, IndexPageIter,
};

const PAGE_SIZE: usize = 4096;
const INTERVAL_LI: [u16; 5] = [1, 2, 3, 16, 64];
/// 随机轮数与每轮 key 数增量。
const ROUND: usize = 8;
const KEY_STEP: usize = 30;

/// 可复现的 xorshift，避免引入随机数依赖。
struct Rng(u64);

impl Rng {
  fn next(&mut self) -> u64 {
    self.0 ^= self.0 << 13;
    self.0 ^= self.0 >> 7;
    self.0 ^= self.0 << 17;
    self.0
  }

  fn below(&mut self, n: u64) -> u64 {
    self.next() % n
  }
}

/// 随机生成共享前缀较多的有序去重 key。
fn rand_key_li(rng: &mut Rng, n: usize) -> Vec<Vec<u8>> {
  let mut key_li: Vec<Vec<u8>> = (0..n)
    .map(|_| {
      let len = 1 + rng.below(6) as usize;
      (0..len).map(|_| b'a' + rng.below(4) as u8).collect()
    })
    .collect();
  key_li.sort();
  key_li.dedup();
  key_li
}

/// 探测 key：所有页内 key、各 key 加后缀、以及 key 之外的随机串与极值。
fn probes(rng: &mut Rng, key_li: &[Vec<u8>]) -> Vec<Vec<u8>> {
  let mut probe_li = vec![Vec::new(), vec![0], vec![0xff; 8]];
  for key in key_li {
    probe_li.push(key.clone());
    let mut longer = key.clone();
    longer.push(0);
    probe_li.push(longer);
    probe_li.push(key[..key.len() - 1].to_vec());
  }
  probe_li.extend(rand_key_li(rng, 32));
  probe_li
}

type DataSnap = (Vec<u8>, Vec<u8>, u64, u64, bool, bool, CompressionType);

fn data_snap(iter: &DataPageIter<'_>) -> DataSnap {
  (
    iter.key().to_vec(),
    iter.value().to_vec(),
    iter.timestamp(),
    iter.expire_ts(),
    iter.is_overflow(),
    iter.is_large_value(),
    iter.compression(),
  )
}

/// 构建 data page，返回页与实际写入的 key 数。
fn build_data(rng: &mut Rng, key_li: &[Vec<u8>], interval: u16) -> (Vec<u8>, usize) {
  let mut builder = DataPageBuilder::new(PAGE_SIZE, interval).unwrap();
  let mut cnt = 0;
  for key in key_li {
    let value = vec![b'v'; rng.below(8) as usize];
    // 时间戳乱序，验证 restart 点 ts 为绝对值、区间内 delta 正确。
    let ts = rng.below(1 << 40);
    let expire = if rng.below(3) == 0 {
      rng.below(1 << 30) + 1
    } else {
      0
    };
    let overflow = rng.below(5) == 0;
    let large = !overflow && rng.below(7) == 0;
    let compression = if rng.below(4) == 0 {
      CompressionType::Standalone
    } else {
      CompressionType::None
    };
    if !builder
      .add(EntrySpec {
        key,
        value: &value,
        ts,
        expire_ts: expire,
        overflow,
        compression,
        large_value: large,
      })
      .unwrap()
    {
      break;
    }
    cnt += 1;
  }
  (builder.finish().unwrap(), cnt)
}

fn linear_seek(page: &[u8], target: &[u8]) -> Option<DataSnap> {
  let mut iter = DataPageIter::new(page).unwrap();
  while iter.advance().unwrap() {
    if iter.key() >= target {
      return Some(data_snap(&iter));
    }
  }
  None
}

fn linear_floor(page: &[u8], target: &[u8]) -> Option<DataSnap> {
  let mut iter = DataPageIter::new(page).unwrap();
  let mut last = None;
  while iter.advance().unwrap() && iter.key() <= target {
    last = Some(data_snap(&iter));
  }
  last
}

/// 定位后继续 advance 的序列也要与线性一致（验证 curr_offset/restart_idx）。
fn data_tail(iter: &mut DataPageIter<'_>) -> Vec<DataSnap> {
  let mut out = Vec::new();
  while iter.advance().unwrap() {
    out.push(data_snap(iter));
  }
  out
}

fn linear_tail(page: &[u8], from: &[u8]) -> Vec<DataSnap> {
  let mut iter = DataPageIter::new(page).unwrap();
  let mut out = Vec::new();
  while iter.advance().unwrap() {
    if iter.key() > from {
      out.push(data_snap(&iter));
    }
  }
  out
}

#[test]
fn data_seek_matches_linear() {
  let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
  for round in 0..ROUND {
    for interval in INTERVAL_LI {
      let key_li = rand_key_li(&mut rng, 1 + round * KEY_STEP);
      let (page, _) = build_data(&mut rng, &key_li, interval);
      let mut iter = DataPageIter::new(&page).unwrap();
      for probe in probes(&mut rng, &key_li) {
        let want = linear_seek(&page, &probe);
        assert_eq!(iter.seek(&probe).unwrap(), want.is_some());
        assert_eq!(iter.valid(), want.is_some());
        if let Some(want) = want {
          assert_eq!(data_snap(&iter), want, "seek {probe:?} interval {interval}");
          let key = iter.key().to_vec();
          assert_eq!(data_tail(&mut iter), linear_tail(&page, &key));
        }

        let want = linear_floor(&page, &probe);
        assert_eq!(iter.seek_floor(&probe).unwrap(), want.is_some());
        assert_eq!(iter.valid(), want.is_some());
        if let Some(want) = want {
          assert_eq!(
            data_snap(&iter),
            want,
            "floor {probe:?} interval {interval}"
          );
          let key = iter.key().to_vec();
          assert_eq!(data_tail(&mut iter), linear_tail(&page, &key));
        }
      }
    }
  }
}

#[test]
fn data_seek_edges() {
  // 空页
  let page = DataPageBuilder::new(PAGE_SIZE, 4)
    .unwrap()
    .finish()
    .unwrap();
  let mut iter = DataPageIter::new(&page).unwrap();
  assert!(!iter.seek(b"a").unwrap());
  assert!(!iter.seek_floor(b"a").unwrap());
  assert!(!iter.valid());

  // 单 entry
  let mut builder = DataPageBuilder::new(PAGE_SIZE, 4).unwrap();
  builder
    .add(EntrySpec {
      key: b"m",
      value: b"v",
      ts: 7,
      expire_ts: 0,
      overflow: false,
      compression: CompressionType::None,
      large_value: false,
    })
    .unwrap();
  let page = builder.finish().unwrap();
  let mut iter = DataPageIter::new(&page).unwrap();
  assert!(iter.seek(b"a").unwrap());
  assert_eq!(iter.key(), b"m");
  assert!(!iter.seek_floor(b"a").unwrap());
  assert!(iter.seek_floor(b"m").unwrap());
  assert_eq!((iter.key(), iter.timestamp()), (b"m".as_slice(), 7));
  assert!(iter.seek_floor(b"z").unwrap());
  assert_eq!(iter.key(), b"m");
  assert!(!iter.seek(b"n").unwrap());
  assert!(!iter.advance().unwrap());

  // restart key 精确命中、首尾越界
  let mut builder = DataPageBuilder::new(PAGE_SIZE, 2).unwrap();
  for (idx, key) in [b"b", b"d", b"f", b"h", b"j"].iter().enumerate() {
    builder
      .add(EntrySpec {
        key: *key,
        value: b"v",
        ts: 100 - idx as u64,
        expire_ts: 0,
        overflow: false,
        compression: CompressionType::None,
        large_value: false,
      })
      .unwrap();
  }
  let page = builder.finish().unwrap();
  let mut iter = DataPageIter::new(&page).unwrap();
  // f 是第 2 个 restart 点
  assert!(iter.seek(b"f").unwrap());
  assert_eq!((iter.key(), iter.timestamp()), (b"f".as_slice(), 98));
  assert!(iter.seek_floor(b"f").unwrap());
  assert_eq!((iter.key(), iter.timestamp()), (b"f".as_slice(), 98));
  // floor 落在 restart 区间末尾（非 restart entry，ts 走 delta）
  assert!(iter.seek_floor(b"g").unwrap());
  assert_eq!((iter.key(), iter.timestamp()), (b"f".as_slice(), 98));
  assert!(iter.seek_floor(b"e").unwrap());
  assert_eq!((iter.key(), iter.timestamp()), (b"d".as_slice(), 99));
  assert!(!iter.seek_floor(b"a").unwrap());
  assert!(iter.seek_floor(b"zz").unwrap());
  assert_eq!((iter.key(), iter.timestamp()), (b"j".as_slice(), 96));
  assert!(!iter.advance().unwrap());
  assert!(!iter.seek(b"k").unwrap());
}

type IndexSnap = (Vec<u8>, u32);

fn build_index(rng: &mut Rng, key_li: &[Vec<u8>], interval: u16) -> (Vec<u8>, u32) {
  let mut builder = IndexPageBuilder::new(PAGE_SIZE, interval, true).unwrap();
  let leftmost = rng.below(1 << 20) as u32;
  assert!(builder.add(b"", leftmost).unwrap());
  for key in key_li {
    if !builder.add(key, rng.below(1 << 30) as u32).unwrap() {
      break;
    }
  }
  (builder.finish().unwrap(), leftmost)
}

fn index_linear(page: &[u8], pred: impl Fn(&[u8]) -> bool, floor: bool) -> Option<IndexSnap> {
  let mut iter = IndexPageIter::new(page).unwrap();
  let mut last = None;
  while iter.advance().unwrap() {
    let hit = pred(iter.key());
    if !floor && hit {
      return Some((iter.key().to_vec(), iter.page_id()));
    }
    if floor {
      if !hit {
        break;
      }
      last = Some((iter.key().to_vec(), iter.page_id()));
    }
  }
  last
}

#[test]
fn index_seek_matches_linear() {
  let mut rng = Rng(0x2545_f491_4f6c_dd1d);
  for round in 0..ROUND {
    for interval in INTERVAL_LI {
      let key_li = rand_key_li(&mut rng, round * KEY_STEP);
      let (page, _) = build_index(&mut rng, &key_li, interval);
      let mut iter = IndexPageIter::new(&page).unwrap();
      for probe in probes(&mut rng, &key_li) {
        let want = index_linear(&page, |k| k >= probe.as_slice(), false);
        assert_eq!(iter.seek(&probe).unwrap(), want.is_some());
        if let Some(want) = want {
          assert_eq!((iter.key().to_vec(), iter.page_id()), want);
        }
        let want = index_linear(&page, |k| k <= probe.as_slice(), true);
        assert_eq!(iter.seek_floor(&probe).unwrap(), want.is_some());
        if let Some(want) = want {
          assert_eq!((iter.key().to_vec(), iter.page_id()), want);
          // 定位后继续推进与线性一致
          let key = iter.key().to_vec();
          let mut rest = Vec::new();
          while iter.advance().unwrap() {
            rest.push((iter.key().to_vec(), iter.page_id()));
          }
          let mut lin = IndexPageIter::new(&page).unwrap();
          let mut want_rest = Vec::new();
          while lin.advance().unwrap() {
            if lin.key() > key.as_slice() {
              want_rest.push((lin.key().to_vec(), lin.page_id()));
            }
          }
          assert_eq!(rest, want_rest);
        }
      }
    }
  }
}

#[test]
fn index_seek_edges() {
  // 只有 leftmost：无 entry
  let mut builder = IndexPageBuilder::new(PAGE_SIZE, 4, false).unwrap();
  builder.add(b"", 1).unwrap();
  let page = builder.finish().unwrap();
  let mut iter = IndexPageIter::new(&page).unwrap();
  assert!(!iter.seek(b"a").unwrap());
  assert!(!iter.seek_floor(b"a").unwrap());
  assert!(!iter.valid());

  let mut builder = IndexPageBuilder::new(PAGE_SIZE, 1, true).unwrap();
  builder.add(b"", 1).unwrap();
  for (key, id) in [(b"b", 20u32), (b"d", 10), (b"f", 30)] {
    builder.add(key, id).unwrap();
  }
  let page = builder.finish().unwrap();
  let mut iter = IndexPageIter::new(&page).unwrap();
  assert!(!iter.seek_floor(b"a").unwrap());
  assert!(iter.seek_floor(b"d").unwrap());
  assert_eq!((iter.key(), iter.page_id()), (b"d".as_slice(), 10));
  assert!(iter.seek_floor(b"zz").unwrap());
  assert_eq!((iter.key(), iter.page_id()), (b"f".as_slice(), 30));
  assert!(iter.seek(b"c").unwrap());
  assert_eq!((iter.key(), iter.page_id()), (b"d".as_slice(), 10));
  assert!(!iter.seek(b"g").unwrap());
}
