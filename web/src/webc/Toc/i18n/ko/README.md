# 디렉터리 탐색 구성 요소

- **반응형 서랍**: 넓은 화면에서 사이드바를 표시하고 좁은 화면에서는 자동으로 축소되며 `c-side` 서랍 및 제목 트리거 버튼과 협력합니다.
- **다국어 드라이버**: 개체 사전 또는 다중 언어 기능을 지원하고 언어 전환에 자동으로 응답합니다.
- **자동 하위 디렉터리**: 대상 컨테이너의 `h1`~`h6` 제목을 자동으로 구문 분석하여 보조 디렉터리를 생성하고 해시 뷰포트 스크롤 강조 표시를 모니터링합니다.
- **비동기 로딩**: 디렉토리 항목을 클릭하면 마크다운 텍스트를 비동기적으로 자동으로 가져와 콘텐츠 영역에 삽입하고 `Ing` 로딩 애니메이션을 표시합니다.

## 데모 사용

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "구성요소 사용"
});
</script>
```

## 인터페이스 매개변수

### 속성 및 방법

- `toc.prefix`: 라우팅 접두사 문자열(예: `"doc"`, 후행 슬래시는 선택 사항)을 가져오거나 설정하면 `README` 키가 자동으로 빈 경로(예: 접두사 자체)에 매핑됩니다.
- `toc.li`: 디렉토리 다국어 생성 기능 가져오기 또는 설정 `(lang) => ({ key: title })`
- `toc.mdUrl`: 마크다운 URL 생성 기능 가져오기 또는 설정 `(key, lang) => url`
- `toc.target`: 연관된 대상 렌더링 노드(예: `c-md`)를 가져오거나 설정합니다. 지정하지 않으면 자동으로 `c-md` / `main` / `article`를 찾습니다.
- `toc.load(key)`: 키에 해당하는 문서 로드 및 디렉토리 확장을 수동으로 트리거합니다.
- `toc.sync()`: 현재 대상 노드의 제목을 수동으로 다시 구문 분석하고, 보조 디렉토리를 동기화하고 상태를 강조 표시합니다.