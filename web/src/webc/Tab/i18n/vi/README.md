# Các tab có thể chuyển đổi

- Trạng thái đã chọn của nhãn được tích hợp liền mạch với khu vực nội dung bên dưới.
- Thông số cấu trúc: Các phần tử con được cố định để sử dụng `nav > a` làm điều hướng nhãn và phần tử con trực tiếp `b` được sử dụng làm vùng nội dung.
- Tự động liên kết thẻ `a[value]` với nút nội dung `b[slot]`.
- Hỗ trợ tự động duy trì trang đang hoạt động hiện tại thành `localStorage` thông qua thuộc tính `key`.
- Hỗ trợ chỉ định trang tab được kích hoạt thông qua thuộc tính `value` (nếu không có bộ đệm và `value`, thẻ có `class="A"` hoặc thẻ đầu tiên sẽ được chọn theo mặc định).
- Triển khai Light DOM nhẹ, không bị cản trở Shadow DOM, mức độ tự do cao trong việc tùy chỉnh kiểu dáng.

## Sử dụng bản trình diễn

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">quả táo</a>
    <a value="banana">chuối</a>
    <a value="orange">quýt</a>
  </nav>
  <b slot="apple">Táo là loại trái cây bổ dưỡng</b>
  <b slot="banana">Chuối là loại trái cây nhiệt đới</b>
  <b slot="orange">Cam rất giàu vitamin C</b>
</c-tab>
```

## tài sản

- `key`: tên khóa được sử dụng để duy trì localStorage
- `value`: giá trị hiện được chọn

## Sự kiện tùy chỉnh

- `change`: Được kích hoạt khi chuyển tab, `e.value` là giá trị `value` của thẻ hiện được kích hoạt.