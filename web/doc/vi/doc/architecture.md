# Ngành kiến ​​​​trúc

## thời gian chạy

Mỗi phân đoạn có một luồng hệ điều hành độc quyền và thời gian chạy compio. Yêu cầu đi qua hàng đợi vào,
Nhập học bằng máy trạng thái phân đoạn, hoàn thành thông qua biên nhận oneshot.

## kho

Công cụ sử dụng cây B+copy-on-write. trang dữ liệu, trang chỉ mục, chuỗi tràn và
Tệp kê khai được tuần tự hóa thành các tệp phân đoạn. nhánh được xác định bởi `(branch, term)` và được nắm giữ bởi mỗi
bản kê khai và ảnh chụp nhanh ánh xạ.

## IO

`fkv_io` Xác định đặc điểm IO không đồng bộ. `fkv_io_compio` Được triển khai với compio + io_uring,
`fkv_io_mem` / `fkv_io_sim` cung cấp phần phụ trợ bộ nhớ và chèn lỗi, `fkv_object_s3` cung cấp
Chương trình phụ trợ đối tượng S3.

## kiên trì

Tệp kê khai ghi lại trang gốc, gốc TTL, siêu dữ liệu nhánh và ánh xạ. Khi mở lại
Trước tiên, hãy phát lại tệp kê khai, sau đó xây dựng lại trình ánh xạ trang rồi bắt đầu phân phát yêu cầu.