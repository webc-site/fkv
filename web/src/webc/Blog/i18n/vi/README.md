# Thành phần bài đăng trên blog

- **Nhóm năm tự động**: Tự động phân loại năm theo ngày viết bài và sắp xếp chúng theo thứ tự ngược lại theo năm
- **Định dạng ngày đơn giản**: Tự động loại bỏ tiền tố năm khi hiển thị và chỉ hiển thị tháng và ngày
- **Tóm tắt và văn bản đánh dấu**: Bản xem trước tóm tắt danh sách tích hợp sẵn và hiển thị văn bản hoàn chỉnh của một bài viết
- **Tiêu đề ba chiều gradient**: Tích hợp `H1Txt` để hiển thị tiêu đề và ngày phát hành

## Sử dụng bản trình diễn

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/xuất bản": ["tiêu đề", "tóm tắt", "27-07-2026"]
};

// Chuyển sang tải nội dung bài viết đã chỉ định (truyền một chuỗi trống để chuyển về danh sách)
blog.load("2026/publish");
</script>
```

## Thông số giao diện

### Thuộc tính và phương thức

- `blog.prefix`: Nhận hoặc đặt tiền tố liên kết bài viết (chẳng hạn như `"/blog/"`)
- `blog.mdUrl`: Nhận hoặc đặt chức năng tạo địa chỉ bài viết Markdown `(key, lang) => url`
- `blog._`: Nhận hoặc đặt từ điển siêu dữ liệu danh sách bài viết `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Chuyển sang tải nội dung bài viết theo key quy định, truyền 1 chuỗi trống `""` để chuyển về danh sách blog