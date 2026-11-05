# Các nút và cửa sổ bật lên để chuyển đổi ngôn ngữ giao diện

- Nhấp vào nút để bật lên lớp nổi lựa chọn ngôn ngữ.
- Tự động đánh dấu ngôn ngữ hiện được chọn.

## Sử dụng bản trình diễn

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Sử dụng mã JS sau để theo dõi sự thay đổi ngôn ngữ
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Thông số giao diện

### Thẻ tùy chỉnh `<c-i18n>`

Thẻ HTML tùy chỉnh, được giới thiệu trong JS và được sử dụng trong HTML.

## Quản lý trạng thái

Sử dụng `src/js/i18n.js` để quản lý trạng thái ngôn ngữ chung:

- **`LANG_LI`[[tên, ID], ...]`.
- **<b id="1"**: Lấy ID ngôn ngữ hiện tại.
- **<b id="1"**: Lấy mã hóa ngôn ngữ hiện tại.
- **<b id="1"**: Đặt ID ngôn ngữ và thông báo cho người đăng ký.
- ***<b id="1"**: Đăng ký thay đổi ngôn ngữ. Nếu ngôn ngữ đã được đặt, lệnh gọi lại sẽ được kích hoạt ngay lập tức. Trả về chức năng hủy đăng ký.

## lớp phong cách

### `.BtnC.lang`

nút biểu tượng.

### `.I18n.Lg`

Bố cục chính của cửa sổ bật lên được sử dụng để bao bọc các tùy chọn ngôn ngữ.