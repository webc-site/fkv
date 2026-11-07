# Bài kiểm tra

## địa phương

```bash
cargo nextest run --workspace --all-features
./test.sh
```

`fkv_failpoint` Sổ đăng ký toàn cầu được chia sẻ theo trường hợp sử dụng; thêm khi sử dụng trực tiếp `cargo test`
`--test-threads=1`.

## Vùng chứa Linux

```bash
cargo install cargo-zigbuild
./docker-test.sh
./docker-test.sh -p fkv_store
```

Tập lệnh sử dụng zigbuild để biên dịch chéo tệp nhị phân thử nghiệm Linux, sau đó đặt nó vào hình ảnh Debian tối thiểu để chạy.

## CI

`ci.yml` Chạy các bài kiểm tra fmt, clippy, đầy đủ và kiểm tra tích hợp RustFS S3.