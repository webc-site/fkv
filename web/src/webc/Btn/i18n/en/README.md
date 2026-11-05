# Ordinary button and round icon button

## Class name

- Normal button`.Btn`
- icon button `.BtnC`
- Primary highlight state`.Btn.Main`

## Use

Add a class name to the element:

```html
<!-- Normal button -->
<button class="Btn">Confirm</button>

<!-- Main Highlight -->
<button class="Btn Main">Submit</button>

<!-- Link Button -->
<a class="Btn" href="https://github.com">Link Button</a>
```

## Icon Style

The icon for `BtnC` can be customized via `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```