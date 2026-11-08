# 일반버튼과 동그란 아이콘버튼

## 수업명

- 일반 버튼`.Btn`
- 아이콘 버튼 `.BtnC`
- 기본 하이라이트 상태`.Btn.Main`

## 사용

요소에 클래스 이름을 추가합니다.

```html
<!-- 일반 버튼 -->
<button class="Btn">확인하다</button>

<!-- 주요 하이라이트 -->
<button class="Btn Main">제출하다</button>

<!-- 링크 버튼 -->
<a class="Btn" href="https://github.com">링크 버튼</a>
```

## 아이콘 스타일

`BtnC` 아이콘은 `background-image`을 통해 맞춤 설정할 수 있습니다.

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```