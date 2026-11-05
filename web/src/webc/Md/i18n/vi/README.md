# Phân tích cú pháp và hiển thị văn bản Markdown thành HTML một cách an toàn.

- **Sạch hóa bảo mật**: Bảo vệ chống lại các cuộc tấn công XSS bằng cách sử dụng trình dọn dẹp `setHTML` API hoặc `DOMParser` gốc
- **Trích xuất nội dung**: Tự động đọc nội dung văn bản của vị trí mặc định trong quá trình khởi tạo và hiển thị nó
- **Tính năng sắp chữ tích hợp**: danh sách đặt trước, bảng, khối mã, kiểu hộp trích dẫn và cảnh báo

## Sử dụng bản trình diễn

### kết xuất tĩnh

```html
<c-md>
# tiêu đề
Đây là một phần nội dung Markdown
</c-md>
```

### phân công động

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Tiêu đề mới\nNội dung mới";
</script>
```

## Thông số giao diện

Truyền văn bản Markdown trực tiếp vào thẻ (dưới dạng vị trí mặc định) hoặc cập nhật động qua `value`.

## Mô tả phong cách

Lớp kiểu `.Md` được tự động thêm khi thành phần được khởi tạo. Hỗ trợ bố cục hộp cảnh báo mở rộng Markdown sau (được kích hoạt bằng cú pháp tham chiếu):

- `[!NOTE]`:Thông tin nhắc nhở
- `[!TIP]`: Gợi ý/Mẹo
- `[!IMPORTANT]`: Chú ý quan trọng
- `[!WARNING]`: Cảnh báo rủi ro
- `[!CAUTION]`: Cảnh báo rủi ro cực cao

## cơ chế bảo mật

Khi kết xuất, trước tiên hãy gọi API `setHTML` gốc để chèn HTML một cách an toàn. Nếu trình duyệt không hỗ trợ, nó sẽ tự động hạ cấp xuống trình dọn dẹp đệ quy dựa trên `DOMParser` và thực hiện chính sách bảo mật sau:
- Xóa các thẻ không an toàn (ví dụ: `script`, `iframe`, v.v.)
- Xóa thuộc tính sự kiện `on*`
- Bảo vệ chống lại các cuộc tấn công XSS bằng cách lọc các thuộc tính bắt đầu bằng các giao thức nguy hiểm như `javascript:`, `vbscript:` và `data:` (ngoại trừ `data:image/`).