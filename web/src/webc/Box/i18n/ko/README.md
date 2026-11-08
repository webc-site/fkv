# 팝업 레이어 생성 및 표시

- 팝업 레이어를 생성하고 표시하는 함수 호출
- 닫힐 때 DOM에서 요소를 자동으로 제거합니다.
- Esc 키 취소 동작 방지

## 데모 사용

```html
<button>팝업하려면 클릭하세요.</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "제목";
  Description.textContent = "텍스트";
  button.className = "Btn Main";
  버튼.textContent = "확인";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## 인터페이스 매개변수

### 기본으로 내보낸 함수

`Box()`

- **반환값**: `HTMLDialogElement`, `Box` 스타일 클래스의 다이얼로그 요소가 추가되었습니다.

## 스타일 수업

### `.Box`

위치 지정, 중앙 지정 및 배경 마스크 흐림을 설정하기 위해 `dialog` 요소에 적용됩니다.

### `.Lg`

내부 콘텐츠에 적용되는 스타일 클래스로, 배경색, 둥근 모서리, 그림자 및 수직 레이아웃을 설정하고 내부 `h3` 및 `p` 의 레이아웃을 정의합니다.