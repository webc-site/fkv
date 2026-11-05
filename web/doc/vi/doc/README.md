# fkv

fkv là một công cụ lưu trữ KV nhúng thuần túy của Rust.

## đặc trưng

- Một luồng hệ điều hành trên mỗi phân đoạn, vòng lặp công việc hợp tác.
- Cây B+COW, chỉ nối đoạn IO.
- định dạng trang/chỉ mục/tệp kê khai/tràn/nén.
- phụ trợ đối tượng cục bộ/bộ nhớ/mô phỏng/S3.
- chế độ bản sao độc lập / đám mây / dự phòng.
- Không có C ABI, không có Python SDK; chỉ có Rust là giao diện công cộng.

## Cổng vào

- [Ngành kiến ​​​​trúc](architecture.md)
- [Crate](crates.md)
- [Bài kiểm tra](testing.md)
- [hiệu suất](performance.md)