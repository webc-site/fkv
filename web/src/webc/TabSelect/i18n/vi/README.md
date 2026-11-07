# Trang tab có lựa chọn thả xuống

- Hỗ trợ nhúng hộp chọn thả xuống có chiều rộng thích ứng (`<c-select>`) trong nhãn, hộp này sẽ chỉ được hiển thị khi trang tab hiện tại được kích hoạt.
- Đặc tả cấu trúc: Phần tử con sử dụng `nav > a` làm điều hướng nhãn và phần tử con trực tiếp `b` đóng vai trò là vùng nội dung.
- Tự động liên kết thẻ `a[value]`, `a` với `c-select` (dựa trên thuộc tính giá trị của nhóm và giá trị tùy chọn đã chọn) và nút nội dung `b[slot]`.
- Hỗ trợ tự động duy trì mục hiện được kích hoạt thành `localStorage` thông qua thuộc tính `key`.
- Hỗ trợ tự động lưu và khôi phục các lựa chọn phụ lịch sử cho từng nhóm vào `localStorage` bằng cách sử dụng `key + '/' + tab`.
- Hỗ trợ chỉ định tab đang hoạt động thông qua thuộc tính `value`.

## Sử dụng bản trình diễn

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>ngôn ngữ</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>hệ thống</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript là một ngôn ngữ kịch bản</b>
    <b slot="rs">Rust là ngôn ngữ lập trình cấp hệ thống</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS là một hệ điều hành đồ họa dựa trên Unix</b>
    <b slot="win">Windows là hệ điều hành được phát triển bởi Microsoft</b>
  </b>
</c-tab-select>
```

## tài sản

- `key`: tên khóa được sử dụng để duy trì localStorage
- `value`: giá trị hiện được chọn

## Sự kiện tùy chỉnh

- `change`: Được kích hoạt khi chuyển tab, `e.value` là giá trị `value` của tùy chọn hiện được kích hoạt.