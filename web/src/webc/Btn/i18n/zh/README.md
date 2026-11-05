# 普通按钮与圆形图标按钮

## 类名

- 普通按钮 `.Btn`
- 图标按钮 `.BtnC`
- 主要高亮状态 `.Btn.Main`

## 使用

为元素添加类名：

```html
<!-- 普通按钮 -->
<button class="Btn">确认</button>

<!-- 主要高亮 -->
<button class="Btn Main">提交</button>

<!-- 链接按钮 -->
<a class="Btn" href="https://github.com">链接按钮</a>
```

## 图标样式

可通过 `background-image` 自定义 `BtnC` 的图标：

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```