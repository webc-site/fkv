# Lựa chọn thả xuống, chiều rộng thích ứng với mục hiện tại

Độ rộng của `select` gốc được xác định bởi văn bản tùy chọn dài nhất theo mặc định. Khi chuyển đổi các tùy chọn ngắn, có quá nhiều khoảng trắng, ảnh hưởng đến hình thức.

Thành phần này bao phủ một cách rõ ràng `select` gốc phía trên thẻ `b`, cho phép chiều rộng điều chỉnh chính xác theo độ dài của văn bản hiện được chọn.

- Duy trì kiểu menu thả xuống `select` gốc và hỗ trợ khả năng truy cập trên tất cả các thiết bị đầu cuối
- Hỗ trợ các hoạt động tương tác gốc như điều hướng tab bàn phím hoàn chỉnh và chuyển đổi phím điều hướng.
- Điều chỉnh độ rộng cho phù hợp với tùy chọn hiện tại để loại bỏ khoảng trắng dư thừa

## Sử dụng bản trình diễn

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```