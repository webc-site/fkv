# 측면에서 미끄러지는 서랍 패널

- 내부 스크롤 구조 자동 생성
- 외부 영역을 클릭하면 자동으로 접힙니다.
- 닫으려면 왼쪽 및 오른쪽으로 스와이프하세요.
- 왼쪽 및 오른쪽 방향의 측면 슬라이딩 위치 지정
- 오픈 및 클로징 이벤트 제공

## 데모 사용

```html
<button class="open-btn">열려 있는</button>

<c-side class="right">
  <div>서랍 내용</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## 인터페이스

### 방법

- `open(el)`: 내보내기 기능, 서랍 열기
- `close(el)`: 내보내기 기능, 서랍 닫기
- `el.open()`: 인스턴스 메소드, 서랍 열기
- `el.close()`: 인스턴스 메소드, 서랍 닫기

### 이벤트

- `open`: 켜져 있으면 실행됩니다.
- `close`: 닫히면 실행됩니다.

## 스타일 수업

- `.right`: 오른쪽 슬라이드 아웃 위치 지정
- `.top`: 슬라이드 아웃 위치 지정
- `.bottom`: 위치 지정에서 슬라이드 아웃
- (설정하지 않을 경우 기본값은 왼쪽으로 슬라이드 아웃되는 것입니다)
- `.open`: 공개 상태
- `.H`: 숨겨진 상태
- `.T`: 애니메이션 전환 상태