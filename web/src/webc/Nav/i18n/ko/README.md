# 페이지 스크롤 시 자동으로 접히거나 펼쳐지는 상단 탐색 모음

- **자동 숨기기**: 페이지를 아래로 스크롤하면 자동으로 숨겨집니다.
- **자동 확장**: 페이지를 위로 스크롤하거나 스크롤 막대가 사라지면 자동으로 표시됩니다.
- **성능 최적화**: `requestAnimationFrame`을 사용하여 스크롤을 조절하세요.
- **리소스 해제**: 구성 요소가 삭제되면 이벤트 리스너와 관찰자가 자동으로 제거됩니다.

## 데모 사용

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">링크</a>
      </c-nav>
    </nav>
    <div class="content">내비게이션을 숨기려면 아래로 스와이프하고, 표시하려면 위로 스와이프하세요.</div>
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

## 사용 지침

`<c-vs>`의 하위 요소 또는 하위 요소로 사용해야 합니다.

## 스타일 상태

구성 요소는 `nav.Nav`에서 다음 클래스 이름을 전환합니다.

- `D`: 상태를 표시합니다(아래 또는 상단으로 슬라이드).
- `I`: 전환 상태가 숨겨져 있습니다.
- `H`: 숨겨진 상태.

## 스타일 변수

- `--top`: 구성 요소에 의해 음수 높이 값으로 자동 업데이트되며 위치 지정 및 전환에 사용됩니다.