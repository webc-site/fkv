# Crate

| 층 | Crate | 책임 |
|---|---|---|
| foundation | `fkv_types` | ID, 패턴, 상수 |
| foundation | `fkv_error` | 오류 모델 |
| foundation | `fkv_codec` | LE / varint / XXH3 / bit flags |
| foundation | `fkv_config` | 구성 및 INI 구문 분석 |
| foundation | `fkv_metrics` | 카운터 및 히스토그램 |
| foundation | `fkv_failpoint` | 결함 주입 |
| format | `fkv_page` | data / index page |
| format | `fkv_manifest` | manifest record |
| format | `fkv_compress` | 원시/사전 압축 |
| core | `fkv_btree` | COW B+tree |
| core | `fkv_buf` | 버퍼 정렬 |
| core | `fkv_mapping` | page mapper |
| core | `fkv_large` | 큰 가치와 세그먼트 |
| io | `fkv_io` | 비동기 IO 특성 |
| io | `fkv_io_compio` | 컴피오 백엔드 |
| io | `fkv_io_mem` | 메모리 백엔드 |
| io | `fkv_io_sim` | 시뮬레이션 백엔드 |
| io | `fkv_object` | 객체 스토리지 특성 |
| io | `fkv_object_s3` | S3 백엔드 |
| io | `fkv_standby` | 대기 사본 |
| engine | `fkv_runtime` | shard runtime |
| engine | `fkv_shard` | 작업 루프 및 액세스 |
| engine | `fkv_store` | 매장 수명주기 및 라우팅 |
| engine | `fkv_cloud` | cloud overlay |
| engine | `fkv_gc` | file / local GC |
| engine | `fkv_recovery` | 재생하고 다시 열다 |
| facade | `fkv` | curated facade |
| facade | `fkv_sdk` | put/get/delete/scan |
| facade | `fkv_cli` | CLI |
| perf | `fkv_bench` | 엔드투엔드 벤치마크 |