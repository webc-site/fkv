# Kiểu chữ được tối ưu hóa cho trải nghiệm đọc

- **Sắp chữ cơ bản**: Thích ứng với tiêu đề, đoạn văn, in đậm, in nghiêng, gạch ngang, mã nội tuyến, hình ảnh và đường phân chia.
- **Hình động liên kết**: Hiển thị hình động gạch chân khi di chuột.
- **Danh sách dự án**: Danh sách không có thứ tự sử dụng dấu đầu dòng, danh sách có thứ tự sử dụng số sê-ri nền được làm tròn và hỗ trợ lồng ghép nhiều cấp độ.
- **Danh sách nhiệm vụ**: Cung cấp kiểu hộp kiểm, hỗ trợ trạng thái việc cần làm và đã hoàn thành.
- **Mẹo cảnh báo**: Hỗ trợ năm loại hộp cảnh báo: Lưu ý, Mẹo, Quan trọng, Cảnh báo và Chú ý.
- **Bảng và Mã**: Cung cấp tính năng đánh dấu di chuột qua hàng trong bảng và các khối mã có bóng và nền chuyển màu.

## Sử dụng bản trình diễn

```html
<div class="Md">
  <h1>tiêu đề</h1>
  <p>Văn bản đoạn văn và <a href="#">liên kết</a></p>
  <hr>
  <h2>Danh hiệu cấp hai</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Mục nhiệm vụ</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Nội dung ghi chú</p>
  </blockquote>
</div>
```

## lớp phong cách

Kiểu được áp dụng cho các thành phần HTML trong vùng chứa `.Md`.

### Kiểu hộp cảnh báo

Thêm tên lớp sau vào phần tử `blockquote` với tên lớp `.q` để chuyển đổi hộp cảnh báo:

- `note`: Ghi chú
- `tip`: Mẹo
- `important`: Quan trọng
- `warning`: cảnh báo
- `caution`: Lưu ý