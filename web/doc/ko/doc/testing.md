# 시험

## 현지의

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` 사용 사례 공유 글로벌 레지스트리; `cargo test`를 직접 사용할 때 추가
`--test-threads=1`.

## Linux 컨테이너

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

스크립트는 zigbuild를 사용하여 Linux 테스트 바이너리를 크로스 컴파일한 다음 실행할 최소 Debian 이미지에 넣습니다.

## CI

`ci.yml` fmt, Clippy, 전체 테스트 및 RustFS S3 통합 테스트를 실행하세요.
`linux-test.yml` Ubuntu 24.04 LTS에서 매일 밤 반환되었으며 4KiB 페이지가 표시되었습니다.