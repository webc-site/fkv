# Hiệu ứng làm nổi bật cạnh và nền kính mờ

Đường viền bán trong suốt và bóng bên trong tạo ra các điểm nổi bật ở cạnh phù điêu vi mô.
Là kiểu phụ thuộc cơ bản của các thành phần nút và hộp nhập liệu.

## Sử dụng bản trình diễn

```html
<main class="Lg">
  <article>
    <h3>tiêu đề</h3>
    <pre>nội dung văn bản</pre>
  </article>
</main>
```

## lớp phong cách

### `.Lg`
Thẻ phong cách cơ bản. Áp dụng độ mờ `backdrop-filter`, đường viền nổi vi mô `border` và bóng bên trong `box-shadow`. Áp dụng bộ lọc `svg/glass.svg#h` khi di chuột.

### `main.Lg`
Vùng chứa khối chính của trang. Các góc được làm tròn `24px` với phần đệm phản hồi (`16px` nếu chiều rộng nhỏ hơn `800px`, `24px` nếu không thì).

### `.Btn`, `.BtnC`, `.Input`
Tên lớp thành phần cơ bản. Sau khi giới thiệu phong cách này, bạn sẽ tự động có được bố cục linh hoạt, các mối quan hệ phân cấp và các đặc tính cơ bản của kính mờ.