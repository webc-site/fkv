# Hiệu suất

`fkv_bench` Kiểm tra tải ghi, đọc và quét trên một không gian khóa cố định.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Các chỉ số bao gồm thông lượng, độ trễ ở từng giai đoạn và RSS cao nhất. phát hành quy trình làm việc phát hành x86_64 /
`fkv_bench` nhị phân cho aarch64.