# thanh cuộn ảo

- **Cuộn hai chiều**: Cung cấp tính năng cuộn dọc (`c-vs`) và ngang (`c-hs`).
- **Phản hồi kích thước**: Theo dõi sự thay đổi nội dung và kích thước khu vực, tự động cập nhật kích thước và vị trí thanh cuộn, đồng thời tự động ẩn khi không bị tràn.
- **Hỗ trợ tương tác**: Hỗ trợ cuộn bánh xe, kéo thanh trượt và định vị nhấp chuột theo dõi.
- **Ẩn trạng thái**: Hiển thị và mở rộng thanh trượt khi di chuột hoặc cuộn và tự động ẩn thanh trượt khi đứng yên.

## cuộn dọc

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## cuộn ngang

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## bộ phận phong cách

Tùy chỉnh kiểu của các phần tử bên trong Shadow DOM thông qua `::part()`:

- `::part(scroll)`: Cuộn vùng nội dung.
- `::part(bar)`: Theo dõi thanh cuộn.
- `::part(si)`: Thanh trượt thanh cuộn.
- `::part(i)`: Lớp bao bọc bên trong.
- `::part(drag)`: Track thanh cuộn ở trạng thái kéo.

## lớp phong cách

- `.drag`: Áp dụng cho phần tử `body`, cho biết phần tử đó đang ở trạng thái kéo-cuộn.

## Thuộc tính tùy chỉnh

- `--si-bg`: Màu nền của thanh trượt.
- `--si-anim`: Hoạt ảnh thanh trượt.
- `--cursorScrollhSvg`: Con trỏ chuột khi di chuột qua thanh cuộn ngang.
- `--cursorScrollvSvg`: Con trỏ chuột khi di chuột vào thanh cuộn dọc.