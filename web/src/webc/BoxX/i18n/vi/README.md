# Tạo và hiển thị lớp bật lên bằng nút đóng

- Lệnh gọi chức năng để tạo và hiển thị lớp bật lên
- Nút đóng tích hợp, nhấp để đóng và xóa phần tử khỏi DOM
- Nhấn Esc để làm mờ hộp nhập hoặc đóng lớp bật lên

## Sử dụng bản trình diễn

```html
<button>Bấm để bật lên</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "văn bản";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Thông số giao diện

Hàm xuất mặc định trả về `HTMLDialogElement` và các lớp kiểu `Box` và `X` đã được thêm vào thành phần hộp thoại.

## lớp phong cách

### `.Box.X`

Được áp dụng cho phần tử `dialog`, chỉ định kiểu hộp thoại.

### `a.X`

Áp dụng cho phần tử `a`, chỉ định kiểu nút đóng.