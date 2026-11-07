# Tải và hiển thị danh sách thẻ không đồng bộ

- **Tải không đồng bộ**: Nhận dữ liệu thông qua chức năng không đồng bộ
- **Hiển thị trạng thái**: Hỗ trợ ba trạng thái: đang tải, dữ liệu trống và thẻ kết xuất.
- **Bố cục linh hoạt**: Thẻ được sắp xếp theo hàng

## Sử dụng bản trình diễn

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Tiêu đề " + i, "Mô tả " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Thông số giao diện

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, chức năng tải dữ liệu không đồng bộ. Định dạng của mỗi khóa con là `<tên, giới thiệu, liên kết]`.

## lớp phong cách

- `.CardLi`: phần tử bên ngoài
- `.Card`: phần tử thẻ
- `.Ing`: Đang tải phần tử hoạt hình