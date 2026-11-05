# 온라인으로 HTML 및 JS 코드 디버깅

- 코드 아래에 디버깅 막대를 렌더링하고 CodePen 및 JSFiddle 디버깅 포털을 제공합니다.
- 선택한 디버깅 플랫폼을 자동으로 기억합니다.
- HTML, CSS 및 JS 코드를 동적으로 생성하기 위해 `.gen` 속성을 ​​통해 콜백 주입을 지원합니다.
- `language-js` 클래스 이름으로 하위 요소를 자동으로 식별하고 JS 모드에서 실행

## 데모 사용

```html
<c-htm-demo><pre><code class="language-html">
<h1>데모</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // [html, css, js]를 반환합니다.
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## 인터페이스 매개변수

### 속성 및 방법

- `.gen`: 콜백 함수가 삽입되었습니다. **디버그 표시줄을 표시하려면 이 콜백을 삽입해야 합니다. **
  - 매개변수:
    - `type`: 삽입된 콘텐츠 유형(`HTML = 1`, `JS = 2`).
    - `text`: 하위 요소에서 추출된 코드 텍스트입니다.
  - 반환값: `[htm, css, js]`