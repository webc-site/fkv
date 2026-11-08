# Bảng ngăn kéo trượt ra từ bên cạnh

- Tự động tạo cấu trúc cuộn bên trong
- Bấm vào vùng bên ngoài để tự động thu gọn
- Cử chỉ vuốt sang trái và phải để đóng
- Định vị trượt bên theo hướng trái và phải
- Cung cấp các sự kiện mở và đóng

## Sử dụng bản trình diễn

```html
<button class="open-btn">Mở</button>

<c-side class="right">
  <div>Nội dung ngăn kéo</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## giao diện

### phương pháp

- `open(el)`: Chức năng xuất, mở ngăn
- `close(el)`: Chức năng xuất, đóng ngăn
- `el.open()`: phương thức phiên bản, mở ngăn kéo
- `el.close()`: phương thức phiên bản, đóng ngăn kéo

### sự kiện

- `open`: Kích hoạt khi được bật
- `close`: Kích hoạt khi đóng

## lớp phong cách

- `.right`: định vị trượt ra bên phải
- `.top`: Định vị trượt ra ngoài
- `.bottom`: trượt khỏi vị trí
- (Nếu không đặt thì mặc định là trượt ra bên trái)
- `.open`: trạng thái mở
- `.H`: trạng thái ẩn
- `.T`: trạng thái chuyển tiếp hoạt ảnh