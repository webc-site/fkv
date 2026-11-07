# Sao chép văn bản vào clipboard

Bấm vào nút để sao chép văn bản của phần tử được chỉ định.

- Hỗ trợ sử dụng rel để chỉ định bộ chọn phần tử cần sao chép
- Khi không được chỉ định, phần tử anh chị em trước đó sẽ được sao chép theo mặc định.
- Hộp nhắc nhở Pophover lồng nhau
- Hỗ trợ quốc tế

## Sử dụng bản trình diễn

```html
<!-- Sao chép các phần tử cụ thể -->
<span id="target">Đây là văn bản được sao chép</span>
<c-copy rel="#target"></c-copy>

<!-- Các phần tử trước đó được sao chép theo mặc định -->
<span>văn bản của phần tử trước đó</span>
<c-copy></c-copy>
```

## tham số

| tài sản | minh họa |
| --- | --- |
| rel | Sao chép bộ chọn của phần tử đích. Không bắt buộc |

## phong cách trạng thái

- `c-pophover.Done`: kiểu hộp nhắc nhở sau khi sao chép thành công