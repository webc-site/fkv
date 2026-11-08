# Hiệu suất

`fkv_bench` Kiểm tra tải ghi, đọc và quét trên một không gian khóa cố định.

```bash
cargo run --release -p fkv_bench -- \
  --workload write-read --rows 20000 --batch-size 100
```

Các chỉ số bao gồm thông lượng, độ trễ ở từng giai đoạn và RSS cao nhất. phát hành quy trình làm việc phát hành x86_64 /
`fkv_bench` nhị phân cho aarch64.

## Trận đấu ba chiều với RocksDB/fjall

`crate/perf/fkv_compare` Sử dụng cùng một trình điều khiển giai đoạn và cùng một mã hóa khóa (chỉ số cuối lớn 8 byte ở cuối,
Thứ tự từ điển phù hợp với thứ tự chỉ số dưới) Kiểm tra fkv, RocksDB, fjall: RocksDB đã vượt qua
`crate/perf/fkv_compare/cpp/rocksdb_ffi.cpp` Thư viện chia sẻ đã biên dịch được tải bằng dlopen (cùng C
ABI, bản dựng Rust không liên kết với RocksDB), fjall sử dụng trực tiếp `fjall 3`. Mỗi trong số ba động cơ sẽ mở ra một thư viện.
Tiến lên trên cùng một thư viện theo thứ tự các giai đoạn: tải hàng loạt tạo dữ liệu và sau đó len / đọc điểm / đọc phạm vi /
Tất cả các thao tác xóa đều ảnh hưởng đến cùng một dữ liệu.

```bash
cargo build --release -p fkv_compare
bash scripts/perf/build_rocksdb_ffi.sh # Bỏ qua cột thứ ba nếu không có RocksDB
./target/release/fkv_compare \
  --rows 25000000 --bulk-rows 25000000 --cache-bytes 268435456 \
  --readers 1,4,8,16,32 --workdir /tmp/fkv-compare \
  --rocksdb-lib target/perf/librocksdb_bench_ffi.so \
  --json /tmp/fkv-compare.json
```

Các con số cho ma trận trang chủ chỉ có thể đến từ một lần chạy có thể theo dõi:

```bash
python3 scripts/perf/fkv_compare_to_js.py \
  --json /tmp/fkv-compare.json --perf-js web/src/conf/perf.js \
  --machine "..." --source "CI chạy <id>"
```

GitHub Actions' `perf.yml` thực hiện chính xác điều này (cài đặt `librocksdb-dev`, bộ điều hợp chương trình, chạy
Ma trận hàng 25M, tải lên tạo phẩm và chèn lấp trực tiếp trang khi `backfill=true`).

### cỡ nòng

- **Tỷ lệ dữ liệu/bộ nhớ đệm**: 25.000.000 hàng × 116 B ≈ 2766 MiB dữ liệu, 256 MiB bộ nhớ đệm cho mỗi công cụ,
  ≈ 11:1, phù hợp với đánh giá chính thức của RocksDB về 97 GiB dữ liệu/6 GiB bộ nhớ đệm (≈ 16:1).
- **Cùng một lượng bộ đệm**: bộ đệm trang của fkv (cùng nhóm chỉ mục/dữ liệu), bộ đệm khối của RocksDB
  (`cache_index_and_filter_blocks=true`, kích thước khối 4 KiB), bộ đệm khối riêng của fjall
  Tất cả ngân sách đều được đặt thành cùng một số byte.
- **Tầm cỡ bền bỉ (cột so sánh sử dụng cấu hình mặc định của RocksDB, không căn chỉnh)**: fkv 1×data page + per batch
  1×manifest `fdatasync` (Đẳng cấu với SyncData + Nối thêm bản kê khai của C++ EloqStore,
  Không có công tắc tắt); cột RocksDB sử dụng **cấu hình mặc định** (Bật WAL, `WriteOptions.sync=false`,
  Không có fsync tuần tự), fjall cũng sử dụng mặc định (`PersistMode::Buffer`). Do đó giai đoạn ghi (ghi đơn /
  Sự khác biệt giữa việc ghi theo lô) là **sự khác biệt về cường độ bền bỉ**, không phải sự khác biệt về cách triển khai, điều này phải được nêu trên trang cùng với con số.
  Nếu bạn muốn phân tích "ai nhanh hơn với cùng cường độ đồng bộ hóa", hãy thêm `--align-fsync` (mở RocksDB bằng sync=true,
  fjall sử dụng SyncAll), tùy chọn này không được sử dụng cho các ma trận được xuất bản bên ngoài.
- **Hai trình điều khiển fkv**: `--fkv-driver store` (mặc định, được kết nối trực tiếp với một `Store` duy nhất trong quy trình và
  RocksDB/fjall cũng là các lệnh gọi thư viện) và `--fkv-driver runtime` (chế độ triển khai: một quy trình
  `--fkv-shards` các chuỗi phân đoạn, yêu cầu được định tuyến theo khóa, phù hợp với việc đọc và ghi công khai của C++ EloqStore
  giao diện). Mỗi yêu cầu sau cần phải đi qua hàng đợi + đánh thức đa luồng, gốc (macOS, tệp IO đi qua nhóm luồng)
  Thời gian đọc là khoảng 20 µs/lần, chậm hơn rất nhiều so với kết nối trực tiếp trong quá trình; rẻ hơn nhiều trên Linux/io_uring, nhưng
  Vẫn là một bước nhảy. Ma trận trang sử dụng cỡ nòng lưu trữ và cỡ nòng thời gian chạy được sử dụng cho "hình thức triển khai so với C++
  So sánh "Giao diện công cộng" (đọc luồng đơn thời gian chạy fkv là khoảng 50 kops/s, luồng đơn C++ EloqStore FFI
  mảnh vỡ khoảng 21 kops/s).
- **Đọc tầm cỡ đồng thời**: số chủ đề bên fkv = `min(readers, --read-max-concurrency)`, mặc định
  Giới hạn trên là 4 (phù hợp với vCPU của máy đánh giá), **không** "một luồng cho mỗi đầu đọc" - mức vượt quá chỉ làm tăng hàng đợi;
  Cache per thread = 256 MiB/số thread (tổng dung lượng cố định, không bị băm nhỏ với reader) và được sử dụng
  `FuturesUnordered` Giữ các yêu cầu `--read-window` (mặc định 16) một cách nhanh chóng. Bên RocksDB là
  `readers` Mỗi `std::thread` đọc đồng thời; vòng lặp đơn luồng ở phía fjall (bỏ qua thiết bị đọc,
  Chỉ mang tính chất tham khảo). `--no-fkv-read-threads` có thể thoái hóa trở lại "chuỗi N không đồng bộ xen kẽ trong một thời gian chạy".
  Đó là độ sâu của đường dẫn, không phải đồng thời và không thể so sánh được giữa các công cụ.
- **len()**: Không có công cụ nào trong số ba công cụ này có giao diện đếm O(1). Dòng này là tốc độ đếm đầy đủ của bảng, không phải tra cứu O(1).