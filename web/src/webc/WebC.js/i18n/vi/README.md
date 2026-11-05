# Tài liệu và mã nguồn để hiển thị các thành phần hoặc chức năng công cụ

- Bố cục đáp ứng hai cột, với tài liệu hiển thị ở bên trái và mã nguồn hiển thị ở bên phải
- Hỗ trợ tài liệu và mã nguồn Các nút tắt Chỉnh sửa và Mã để chuyển đến địa chỉ nguồn.
- Tiêu đề và phần giới thiệu được hiển thị ở trên cùng, hỗ trợ dịch tự động i18n.

## Sử dụng bản trình diễn

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Thông số thuộc tính

- `src`: địa chỉ URL của mã nguồn.
- `href`: Địa chỉ URL của phần đánh dấu tài liệu, hỗ trợ sử dụng phần giữ chỗ `{code}` để thích ứng với ngôn ngữ hiện tại.