# 정지 상태에 오른쪽 화살표가 표시된 캡슐 카드

- 오른쪽 화살표를 가리키세요
- 활성 파란색 하이라이트
- 피드백을 확대하려면 클릭하세요.

## 데모 사용

```html
<main style="display:flex;gap:16px">
  <a class="Card A" href="#">
    <h2>제목 1</h2>
    <i>설명 1</i>
  </a>
  <a class="Card" href="#">
    <h2>제목 2</h2>
    <i>설명 2</i>
  </a>
</main>

<script type="module">
import "webc.site/Card.js";
import { On } from "webc.site/js/On.js";

const el = document.querySelector("main"),
  CARD = ".Card",
  A = "A",
  CARD_A = CARD + "." + A;

On(el, {
  click: (e) => {
    const card = e.target.closest(CARD);
    if (card) {
      const active = el.querySelector(CARD_A);
      if (active && active !== card) {
        active.classList.remove(A);
      }
      card.classList.add(A);
    }
  },
});
</script>
```

## 스타일 수업

- `.Card`: 기본 카드
- `.Card.A`: 강조표시 활성화
- `> h2`: 제목
- `> i`: 설명