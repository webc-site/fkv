# Chỉ báo trạng thái tải hoặc xử lý

- Thống nhất con trỏ vào trạng thái chờ
- Hiển thị hoạt ảnh gợn để biểu thị trạng thái xử lý
- Áp dụng mặt nạ làm mờ cho biểu mẫu để ngăn việc gửi trùng lặp

## Sử dụng bản trình diễn

```html
<!-- chỉ báo tải độc lập -->
<div class="Ing"></div>

<!-- Đang tải biểu mẫu có mặt nạ -->
<form class="Ing">
  <input type="text" placeholder="Hộp nhập liệu">
  <button type="submit">nộp</button>
</form>
```

## lớp phong cách

### `.Ing`
- Trạng thái con trỏ là `wait`.
- Phần tử giả `::before` hiển thị hoạt ảnh gợn sóng.
- Khi được sử dụng trên phần tử `form`, phần tử giả `::after` hiển thị mặt nạ mờ 1px, vô hiệu hóa tương tác và chọn văn bản.