# Tự động chuyển đổi menu điều hướng và nút quay lại

- Tự động bọc các phần tử con để tạo menu điều hướng và cấu trúc nút quay lại
- Tự động theo dõi lịch sử định tuyến và tính toán đường trở về trước đó
- Hiển thị menu điều hướng khi lộ trình khớp với mục menu, nếu không thì hiển thị nút quay lại

## Sử dụng bản trình diễn

```html
<c-nav-l>
  <a href="/">trang đầu</a>
  <a href="/doc">tài liệu</a>
</c-nav-l>
```

## lớp phong cách

- `B`: kích hoạt trạng thái nút quay lại. Hiển thị nút quay lại và ẩn menu điều hướng. Tự động chuyển đổi trong thành phần dựa trên định tuyến.