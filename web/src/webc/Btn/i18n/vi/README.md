# Nút thông thường và nút biểu tượng tròn

## Tên lớp

- Nút bình thường`.Btn`
- nút biểu tượng `.BtnC`
- Trạng thái đánh dấu chính`.Btn.Main`

## sử dụng

Thêm tên lớp vào thành phần:

```html
<!-- Nút bình thường -->
<button class="Btn">xác nhận</button>

<!-- điểm nhấn chính -->
<button class="Btn Main">nộp</button>

<!-- nút liên kết -->
<a class="Btn" href="https://github.com">nút liên kết</a>
```

## phong cách biểu tượng

Biểu tượng cho `BtnC` có thể được tùy chỉnh thông qua `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```