# Sự kết hợp của hộp lựa chọn thả xuống và nút

- Ghép hộp chọn và nút theo chiều ngang
- Tự động lưu trạng thái đã chọn vào localStorage
- Chặn các lần nhấp vào nút và gửi các sự kiện tùy chỉnh

## Sử dụng bản trình diễn

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Tùy chọn 1</option>
    <option value="option2">Tùy chọn 2</option>
  </c-select>
  <button>chạy</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## tài sản

- `key`: tên khóa được sử dụng để duy trì localStorage
- `value`: giá trị hiện được chọn

## sự kiện

- `submit`: Được gửi đi khi nút được nhấp vào, `e.value` mang giá trị hiện được chọn