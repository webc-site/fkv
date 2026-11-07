# Kết xuất và hiển thị các bản demo thành phần, tài liệu và hướng dẫn cài đặt

- Trích xuất và phân tích tiêu đề, phần giới thiệu và nội dung của tài liệu README
- Hộp cát để chạy các bản trình diễn tương tác của các thành phần và tách biệt các kiểu
- Chuyển đổi các khối mã trong tài liệu thành bản trình bày tương tác
- Bố cục thích ứng, hiển thị cột màn hình rộng, hiển thị chuyển đổi tab màn hình hẹp

## Sử dụng bản trình diễn

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Khởi tạo và lấy giao diện hoạt động
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Thông số giao diện

### `WebCDemo(el)`

Khởi tạo thành phần.

- **tham số**
  - `el`:`HTMLElement`, vùng chứa thành phần.
- **Giá trị trả về**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Thiết lập mô-đun demo.
  - `setMd(name, md, lang)`: Đặt tên thành phần, nội dung tài liệu README và ngôn ngữ hiện tại.
  - `unMount()`: Dỡ bỏ thành phần và dọn sạch trình xử lý sự kiện.

### Thuộc tính mô-đun demo `mod`

- `CSS`:`string`, đường dẫn biểu định kiểu của thành phần.
- `HTM`:`string`, mẫu HTML của thành phần.
- `default`:`function(root)`, hàm khởi tạo thành phần, nhận vùng chứa gốc bóng.

## lớp phong cách

### `.M`

khu vực nội dung.

### `.L`

Bảng bên trái chứa hướng dẫn và tài liệu.

### `.R`

Bảng bên phải, chứa bản demo.

### `c-nav`

Thanh điều hướng màn hình hẹp.