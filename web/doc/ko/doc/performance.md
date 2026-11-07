# 성능

`fkv_bench` 고정된 키 공간에서 쓰기, 읽기 및 스캔 로드를 테스트합니다.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

표시기에는 처리량, 각 단계의 대기 시간 및 최대 RSS가 포함됩니다. 워크플로우 릴리스 x86_64 릴리스 /
`fkv_bench` aarch64용 바이너리.