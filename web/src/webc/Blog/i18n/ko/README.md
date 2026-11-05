# 블로그 게시물 구성 요소

- **자동 연도 그룹화**: 기사 날짜에 따라 연도를 자동으로 분류하고 연도별 역순으로 정렬합니다.
- **단순화된 날짜 형식**: 표시 시 연도 접두어를 자동으로 제거하고 월과 일만 표시합니다.
- **마크다운 요약 및 텍스트**: 내장된 목록 요약 미리보기 및 단일 기사 전체 텍스트 렌더링
- **그라디언트 3차원 제목**: `H1Txt`을 통합하여 제목과 출시일 표시

## 데모 사용

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["제목", "초록", "2026-07-27"]
};

// 지정된 기사 텍스트를 로드하도록 전환합니다(목록으로 다시 전환하려면 빈 문자열 전달).
blog.load("2026/publish");
</script>
```

## 인터페이스 매개변수

### 속성 및 방법

- `blog.prefix`: 기사 링크 접두사(예: `"/blog/"`) 가져오기 또는 설정
- `blog.mdUrl`: Markdown 기사 주소 생성 기능 가져오기 또는 설정 `(key, lang) => url`
- `blog._`: 기사 목록 메타데이터 사전 가져오기 또는 설정 `{ [key]: [title, summary, date] }`
- `blog.load(key)`: 지정된 키의 기사 콘텐츠를 로드하도록 전환하고, 블로그 목록으로 다시 전환하려면 빈 문자열 `""`를 전달합니다.