# 구성 요소 데모, 문서 및 설치 지침을 렌더링하고 표시합니다.

- README 문서의 제목, 소개, 내용을 추출하고 구문 분석합니다.
- 구성요소의 대화형 데모를 실행하고 스타일을 분리하는 샌드박스
- 문서의 코드 블록을 대화형 프레젠테이션으로 변환
- 적응형 레이아웃, 와이드 스크린 컬럼 디스플레이, 좁은 스크린 탭 전환 디스플레이

## 데모 사용

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//작업 인터페이스 초기화 및 획득
const [setMod, setMd, unMount] = WebCDemo(el);
```

## 인터페이스 매개변수

### `WebCDemo(el)`

구성요소를 초기화합니다.

- **매개변수**
  - `el`:`HTMLElement`, 구성요소 컨테이너.
- **반환 값**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: 데모 모듈을 설정합니다.
  - `setMd(name, md, lang)`: 구성 요소 이름, README 문서 내용 및 현재 언어를 설정합니다.
  - `unMount()`: 구성요소를 언로드하고 이벤트 리스너를 정리합니다.

### 데모 모듈 `mod` 속성

- `CSS`:`string`, 구성 요소의 스타일 시트 경로입니다.
- `HTM`:`string`, 구성요소의 HTML 템플릿.
- `default`:`function(root)`, 구성요소 초기화 함수는 섀도우 루트 컨테이너를 수신합니다.

## 스타일 수업

### `.M`

콘텐츠 영역.

### `.L`

왼쪽 패널에는 지침과 문서가 포함되어 있습니다.

### `.R`

데모가 포함된 오른쪽 패널.

### `c-nav`

좁은 화면 탐색 표시줄.