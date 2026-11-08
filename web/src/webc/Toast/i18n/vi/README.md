# Cửa sổ bật lên nhắc nhở ở cạnh trang

- **Gọi hàm**: Cung cấp giao diện nhắc nhở thông thường và báo lỗi
- **Tự động xếp chồng**: Nhiều mẹo tự động tính toán khoảng cách và xếp chồng theo chiều dọc
- **Tắt theo lịch và thủ công**: Hỗ trợ tắt theo lịch và thủ công

## Sử dụng bản trình diễn

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Nội dung nhắc nhở";
});

toastErr((el) => {
  el.textContent = "Nội dung lỗi";
});

toast(
  (el) => {
    el.textContent = "Tự động đóng sau 99 giây";
  },
  99
);
```

## Thông số giao diện

### toast(render, timeout)

Tạo và hiển thị lời nhắc.

- `render`: `(el) => void`, hiển thị lệnh gọi lại, `el` là thành phần hộp nhắc nhở.
- `timeout`: `Number`, hết thời gian chờ giây. Mặc định 9. Đặt thành 0 để không tự động tắt.
- Giá trị trả về: phần tử hộp nhắc nhở. Hỗ trợ tắt `el.close()`.

### toastErr(render, timeout)

Tạo và hiển thị thông báo lỗi. Các tham số giống như `toast`, với lớp kiểu `.ERR`.

## lớp phong cách

- `.Toast`: Lớp cơ sở của hộp nhắc nhở.
- `.ERR`: Trạng thái lỗi.
- `.animated` / `.fadeInR` / `.fadeOutR`: chuyển tiếp hoạt ảnh.
- `.x`: Nút đóng.