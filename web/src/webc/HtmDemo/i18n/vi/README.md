# Gỡ lỗi mã HTML và JS trực tuyến

- Hiển thị thanh gỡ lỗi bên dưới mã và cung cấp cổng gỡ lỗi CodePen và JSFiddle
- Tự động ghi nhớ nền tảng gỡ lỗi đã chọn
- Hỗ trợ chèn lệnh gọi lại thông qua thuộc tính `.gen` để tự động tạo mã HTML, CSS và JS
- Tự động xác định các phần tử con có tên lớp `language-js` và chạy ở chế độ JS

## Sử dụng bản trình diễn

```html
<c-htm-demo><pre><code class="language-html">
<h1>Thử nghiệm</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Trả về [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Thông số giao diện

### Thuộc tính và phương thức

- `.gen`: Chức năng gọi lại được đưa vào. **Cuộc gọi lại này phải được đưa vào để thanh gỡ lỗi được hiển thị. **
  - tham số:
    - `type`: Loại nội dung được chèn (`HTML = 1`, `JS = 2`).
    - `text`: Văn bản mã được trích xuất từ ​​phần tử con.
  - Giá trị trả về: `[htm, css, js]`