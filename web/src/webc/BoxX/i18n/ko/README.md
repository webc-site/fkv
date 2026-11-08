# 닫기 버튼이 있는 팝업 레이어 생성 및 표시

- 팝업 레이어를 생성하고 표시하는 함수 호출
- 내장된 닫기 버튼, 클릭하여 DOM에서 요소를 닫고 제거합니다.
- 입력 상자의 초점을 흐리게 하거나 팝업 레이어를 닫으려면 Esc 키를 누르세요.

## 데모 사용

```html
<button>팝업하려면 클릭하세요.</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "텍스트";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## 인터페이스 매개변수

기본 내보내기 기능은 `HTMLDialogElement`을 반환하고 `Box` 및 `X` 스타일 클래스가 대화 상자 요소에 추가되었습니다.

## 스타일 수업

### `.Box.X`

`dialog` 요소에 적용되며 대화상자 스타일을 지정합니다.

### `a.X`

`a` 요소에 적용되며 닫기 버튼 스타일을 지정합니다.