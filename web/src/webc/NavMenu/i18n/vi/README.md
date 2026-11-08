# Menu điều hướng thích ứng với các kích thước màn hình khác nhau

- Desktop: Các liên kết được hiển thị theo ô ngang
- Phiên bản di động: Thu gọn vào nút menu, nhấp để trượt ngăn kéo thanh bên
- Kích hoạt tuyến đường: tự động so sánh đường dẫn hiện tại và giá trị băm, đồng thời thêm tên lớp trạng thái kích hoạt vào liên kết phù hợp `A`
- Tự động đóng: Khi thiết bị đầu cuối di động nhấp vào liên kết hoặc nền bên ngoài của thanh bên, thanh bên sẽ tự động đóng lại.

## sử dụng

```html
<c-nav-menu>
  <a href="/">trang đầu</a>
  <a href="/doc">tài liệu</a>
  <a href="/about">Về</a>
</c-nav-menu>
```

## minh họa

Khi thành phần được khởi tạo, nếu không có `c-side` trong phần tử con, nút menu di động và thanh bên `c-side` sẽ được tạo tự động và tất cả các thẻ `a` sẽ được chuyển vào thanh bên.

## lớp phong cách

- **A**: Liên kết được kích hoạt sẽ tự động thêm tên lớp `A`