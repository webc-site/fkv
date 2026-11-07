# Thanh điều hướng trên cùng tự động thu gọn hoặc mở rộng khi cuộn trang

- **Auto Hide**: Tự động ẩn khi cuộn trang xuống.
- **Tự động mở rộng**: Tự động hiển thị khi cuộn trang lên hoặc thanh cuộn biến mất.
- **Tối ưu hóa hiệu suất**: Sử dụng `requestAnimationFrame` để tăng tốc độ cuộn.
- **Giải phóng tài nguyên**: Trình xử lý sự kiện và trình quan sát sự kiện sẽ tự động bị xóa khi thành phần bị hủy.

## Sử dụng bản trình diễn

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">liên kết</a>
      </c-nav>
    </nav>
    <div class="content">Vuốt xuống để ẩn điều hướng, vuốt lên để hiển thị</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Hướng dẫn sử dụng

Phải được sử dụng làm phần tử con hoặc phần tử con của `<c-vs>`.

## trạng thái phong cách

Thành phần này chuyển đổi tên lớp sau trên `nav.Nav`:

- `D`: Hiển thị trạng thái (trượt xuống hoặc lên trên).
- `I`: Trạng thái chuyển tiếp bị ẩn.
- `H`: Trạng thái ẩn.

## biến phong cách

- `--top`: Được thành phần tự động cập nhật thành giá trị chiều cao âm, được sử dụng để định vị và chuyển tiếp.