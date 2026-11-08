# 성능

`fkv_bench` 고정된 키 공간에서 쓰기, 읽기 및 스캔 로드를 테스트합니다.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

표시기에는 처리량, 각 단계의 대기 시간 및 최대 RSS가 포함됩니다. 워크플로우 릴리스 x86_64 릴리스 /
`fkv_bench` aarch64용 바이너리.

## RocksDB/fjall과의 3자 매치업

`crate/perf/fkv_compare` 동일한 스테이지 드라이버와 동일한 키 인코딩을 사용하십시오(끝 부분에 8바이트 빅엔디안 아래 첨자,
사전 순서는 아래 첨자 순서와 일치합니다) 테스트 fkv, RocksDB, fjall: RocksDB 통과
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` 컴파일된 공유 라이브러리는 dlopen(동일 C
ABI, Rust 빌드는 RocksDB에 연결되지 않음), fjall은 `fjall 3`을 직접 사용합니다. 세 엔진 각각은 라이브러리를 엽니다.
동일한 라이브러리를 단계적으로 진행: 대량 로드로 데이터를 생성한 다음 len / 포인트 읽기 / 범위 읽기 /
삭제는 모두 동일한 데이터에 영향을 미칩니다.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # RocksDB가 없으면 세 번째 열을 건너뜁니다.
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

홈 매트릭스의 숫자는 추적 가능한 하나의 실행에서만 나올 수 있습니다.

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI 실행 <id>"
```

GitHub Actions' `perf.yml`이 바로 이 작업을 수행합니다(`librocksdb-dev` 설치, 프로그램 어댑터, 실행
2,500만 행 행렬, 아티팩트 업로드, `backfill=true`)인 경우 페이지를 직접 백필합니다.

### 구경

- **데이터/캐시 비율**: 25,000,000개 행 × 116B ≒ 2766MiB의 데이터, 엔진당 256MiB의 캐시,
  11:1, 97GiB 데이터/6GiB 캐시에 대한 RocksDB의 공식 검토(16:1)와 일치합니다.
- **동일한 양의 캐시**: fkv의 페이지 캐시(인덱스/데이터 동일한 풀), RocksDB의 블록 캐시
  (`cache_index_and_filter_blocks=true`, 블록 크기 4KiB), fjall 자체 블록 캐시
  예산은 모두 동일한 바이트 수로 설정됩니다.
- **지속성 범위(비교 열은 정렬 없이 RocksDB 기본 구성을 사용함)**: fkv 1×데이터 페이지 + 배치당
  1×manifest `fdatasync` (SyncData와 동형 + C++ EloqStore의 AppendManifest,
  꺼짐 스위치도 없습니다); RocksDB 열은 **기본 구성**(WAL on, `WriteOptions.sync=false`,
  순차적 fsync 없음), fjall은 기본값(`PersistMode::Buffer`)도 사용합니다. 따라서 쓰기 단계(단일 쓰기/
  일괄 쓰기 간의 차이)는 구현 차이가 아닌 **지속성 강도 차이**이므로 페이지에 숫자와 함께 명시해야 합니다.
  "동일한 동기화 강도에서 누가 더 빠른지"를 분석하려면 `--align-fsync`을 추가합니다(rocksDB를 sync=true로 열고,
  fjall은 SyncAll을 사용합니다. 이 옵션은 외부에 게시된 행렬에는 사용되지 않습니다.
- **fkv 드라이버 2개**: `--fkv-driver store`(기본값, 프로세스 내의 단일 `Store`에 직접 연결됨,
  RocksDB / fjall도 라이브러리 호출임) 및 `--fkv-driver runtime`(배포 모드: 프로세스
  `--fkv-shards` 샤드 스레드, 요청은 키로 ​​라우팅되며 C++ EloqStore의 공개 읽기 및 쓰기에 맞춰 정렬됩니다.
  인터페이스). 후자의 각 요청은 대기열 + 스레드 간 깨우기, 기본(macOS, 파일 IO가 스레드 풀을 통과함)을 통과해야 합니다.
  판독 시간은 약 20μs/시간으로, 이는 진행 중인 직접 연결보다 훨씬 느립니다. Linux/io_uring에서는 훨씬 저렴하지만
  아직 점프입니다. 페이지 매트릭스는 저장소 구경을 사용하고 런타임 구경은 "배포 양식 대 C++에 사용됩니다.
  "공개 인터페이스" 비교(fkv 런타임 단일 스레드 읽기는 약 50kops/s, C++ EloqStore FFI 단일 스레드
  샤드 약 21kops/s).
- **동시성 구경 읽기**: fkv 측 스레드 번호 = `min(readers, --read-max-concurrency)`, 기본값
  상한은 "리더당 하나의 스레드"가 아닌 **4(평가 시스템 vCPU에 맞춰 정렬됨)입니다. 초과된 수준은 대기열만 증가시킵니다.
  스레드당 캐시 = 256MiB / 스레드 수(총량은 고정되어 있으며 리더로 파쇄되지 않음), 사용됩니다.
  `FuturesUnordered` `--read-window`(기본값 16) 요청을 즉시 유지합니다. RocksDB 쪽은
  `readers` 각 `std::thread`는 동시에 읽습니다. fjall 측의 단일 스레드 루프(리더 장비 무시,
  참조 열로만 사용하세요). `--no-fkv-read-threads`은 "단일 런타임에서 인터리브된 N개의 비동기 체인"으로 다시 퇴화될 수 있습니다.
  이는 동시성이 아닌 파이프라인 깊이이며 엔진 간에 비교할 수 없습니다.
- **len()**: 세 엔진 중 어느 것도 O(1) 계산 인터페이스를 갖고 있지 않습니다. 이 줄은 O(1) 조회가 아닌 전체 테이블 계산 속도입니다.