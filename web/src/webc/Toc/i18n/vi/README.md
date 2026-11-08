# Thành phần điều hướng thư mục

- **Ngăn kéo đáp ứng**: Hiển thị thanh bên trên màn hình rộng, tự động thu gọn trong màn hình hẹp và hợp tác với ngăn kéo `c-side` nút kích hoạt tiêu đề và ngăn kéo
- **Trình điều khiển đa ngôn ngữ**: Hỗ trợ từ điển đối tượng hoặc chức năng đa ngôn ngữ, tự động phản hồi khi chuyển đổi ngôn ngữ
- **Thư mục con tự động**: Tự động phân tích cú pháp các tiêu đề `h6` trong vùng chứa mục tiêu để tạo thư mục phụ và theo dõi điểm đánh dấu cuộn khung nhìn Hash.
- **Tải không đồng bộ**: Nhấp vào một mục thư mục để tự động lấy văn bản Markdown một cách không đồng bộ và đưa văn bản đó vào khu vực nội dung, hiển thị hình ảnh động đang tải `Ing`

## Sử dụng bản trình diễn

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "sử dụng thành phần"
});
</script>
```

## Thông số giao diện

### Thuộc tính và phương thức

- `toc.prefix`: Nhận hoặc đặt chuỗi tiền tố định tuyến (chẳng hạn như `"doc"`, dấu gạch chéo ở cuối là tùy chọn), khóa `README` sẽ tự động được ánh xạ tới một đường dẫn trống (tức là chính tiền tố)
- `toc.li`: Nhận hoặc đặt chức năng tạo đa ngôn ngữ cho thư mục `(lang) => ({ key: title })`
- `toc.mdUrl`: Nhận hoặc đặt chức năng tạo URL Markdown `(key, lang) => url`
- `toc.target`: Nhận hoặc đặt nút kết xuất mục tiêu được liên kết (chẳng hạn như `c-md`). Nếu không được chỉ định, nó sẽ tự động tìm `c-md` / `main` / `article`
- `toc.load(key)`: Kích hoạt tải tài liệu và mở rộng thư mục theo cách thủ công tương ứng với khóa
- `toc.sync()`: Phân tích lại tiêu đề theo cách thủ công trong nút mục tiêu hiện tại, đồng bộ hóa thư mục phụ và đánh dấu trạng thái