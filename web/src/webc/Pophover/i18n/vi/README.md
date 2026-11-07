# dấu nhắc bong bóng nổi

Hiển thị thông tin chú giải công cụ di chuột khi di chuột hoặc phần tử được kích hoạt.

- Dựa trên Thành phần Web gốc, nhẹ và hiệu suất cao
- Hỗ trợ định vị không gian thích ứng theo bốn hướng (class="top/bottom/left/right")
- Nó sử dụng nền gradient kết cấu kính mờ màu đen và các ký tự màu trắng, với các điểm nhấn đường viền vật lý tinh tế.
- Tính toán thích ứng vị trí tích hợp, tự động điều chỉnh hướng bật lên theo không gian còn lại trong khung nhìn
- Sử dụng `slot="pop"` để hiển thị nội dung lời nhắc nổi

## Sử dụng bản trình diễn

```html
<!-- Tự động tính toán hướng (khuyến khích, không cần khai báo lớp) -->
<c-pophover>
  <button>tự động</button>
  <div slot="pop">Đây là một mẹo bong bóng</div>
</c-pophover>

<!-- hướng cố định -->
<c-pophover class="left">
  <button>Bên trái</button>
  <div slot="pop">Đây là một mẹo bong bóng</div>
</c-pophover>

<c-pophover class="right">
  <button>Phải</button>
  <div slot="pop">Đây là một mẹo bong bóng</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Xuống</button>
  <div slot="pop">Đây là một mẹo bong bóng</div>
</c-pophover>

<c-pophover class="top">
  <button>thượng đẳng</button>
  <div slot="pop">Đây là một mẹo bong bóng</div>
</c-pophover>
```

## Biến CSS

Kiểu tùy chỉnh được hỗ trợ thông qua các biến CSS sau:

| tên biến | giá trị mặc định | minh họa |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Khoảng cách giữa chú giải công cụ bong bóng và phần tử kích hoạt |
| `--pophover-margin` | `24px` | Giới hạn an toàn của cạnh khung nhìn trong quá trình tính toán định vị thích ứng |
| `--pophover-bg-top` | `#18181ce0` | Màu nền bong bóng trên cùng |
| `--pophover-bg-mid` | `#121215e5` | Màu nền của mũi tên ở bên trái và bên phải của bong bóng |
| `--pophover-bg-bottom` | `#0c0c0feb` | Màu của mũi tên phía dưới và phía trên và phía dưới của nền gradient bong bóng |

## Kiểu chú giải công cụ biểu tượng (`.Ico`)

Đối với các nút nhắc nổi loại biểu tượng thuần túy (chẳng hạn như "Chỉnh sửa", "Xóa", v.v.), bạn có thể áp dụng biểu tượng hợp nhất và kiểu di chuột bằng cách thêm tên lớp `.Ico` vào vùng chứa bên ngoài hoặc chính thành phần đó.

### Ví dụ về cấu trúc

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">biên tập</div>
  </c-pophover>
</b>
```

Các thay đổi về bố cục, kích thước (18x18px) và màu di chuột cơ bản đã được thống nhất trong `.Ico > c-pophover > a`. Nhà phát triển chỉ cần chỉ định riêng tệp biểu tượng tương ứng, chẳng hạn như:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```