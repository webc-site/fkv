# Tạo và hiển thị lớp bật lên

- Lệnh gọi chức năng để tạo và hiển thị lớp bật lên
- Tự động xóa các phần tử khỏi DOM khi đóng
- Ngăn chặn hành vi hủy khóa Esc

## Sử dụng bản trình diễn

```html
<button>Bấm để bật lên</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Tiêu đề";
  description.textContent = "Văn bản";
  button.className = "Btn Main";
  nút.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Thông số giao diện

### Các hàm được xuất mặc định

`Box()`

- **Giá trị trả về**: `HTMLDialogElement`, thành phần hộp thoại của lớp kiểu `Box` đã được thêm.

## lớp phong cách

### `.Box`

Áp dụng cho phần tử `dialog` để đặt vị trí, căn giữa và làm mờ mặt nạ nền.

### `.Lg`

Một lớp kiểu được áp dụng cho nội dung bên trong, đặt màu nền, các góc tròn, bố cục bóng và dọc, đồng thời xác định bố cục của `h3` và `p` bên trong.