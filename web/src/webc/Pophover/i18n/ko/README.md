# 떠다니는 버블 프롬프트

마우스를 올리거나 요소가 활성화되면 호버 도구 설명 정보를 표시합니다.

- 네이티브 웹 컴포넌트 기반으로 가볍고 고성능
- 4방향의 적응형 공간 위치 지정 지원(클래스="상단/하단/왼쪽/오른쪽")
- 검은색 반투명 ​​유리 텍스처 그라데이션 배경과 흰색 문자를 사용하며 절묘한 물리적 가장자리 윤곽선이 강조 표시됩니다.
- 내장된 위치 적응형 계산으로 뷰포트의 남은 공간에 따라 팝업 방향을 자동으로 조정합니다.
- 플로팅 프롬프트 콘텐츠를 표시하려면 `slot="pop"`을 사용하세요.

## 데모 사용

```html
<!-- 방향 자동 계산(권장, 클래스 선언 필요 없음) -->
<c-pophover>
  <button>오토매틱</button>
  <div slot="pop">버블팁이에요</div>
</c-pophover>

<!-- 고정된 방향 -->
<c-pophover class="left">
  <button>왼쪽</button>
  <div slot="pop">버블팁이에요</div>
</c-pophover>

<c-pophover class="right">
  <button>오른쪽</button>
  <div slot="pop">버블팁이에요</div>
</c-pophover>

<c-pophover class="bottom">
  <button>아래에</button>
  <div slot="pop">버블팁이에요</div>
</c-pophover>

<c-pophover class="top">
  <button>우수한</button>
  <div slot="pop">버블팁이에요</div>
</c-pophover>
```

## CSS 변수

사용자 정의 스타일은 다음 CSS 변수를 통해 지원됩니다.

| 변수 이름 | 기본값 | 설명하다 |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | 버블 툴팁과 트리거 요소 사이의 공간 |
| `--pophover-margin` | `24px` | 적응형 위치 계산 중 뷰포트 가장자리 안전 여유 |
| `--pophover-bg-top` | `#18181ce0` | 버블 그라데이션 배경 상단 색상 |
| `--pophover-bg-mid` | `#121215e5` | 버블의 왼쪽과 오른쪽에 있는 화살표의 배경색입니다. |
| `--pophover-bg-bottom` | `#0c0c0feb` | 버블 그라데이션 배경의 하단 및 상단 및 하단 화살표 색상 |

## 아이콘 툴팁 스타일(`.Ico`)

순수 아이콘 유형 부동 프롬프트 버튼(예: "편집", "삭제" 등)의 경우 `.Ico` 클래스 이름을 외부 컨테이너 또는 구성 요소 자체에 추가하여 통합 아이콘 및 마우스 오버 스타일을 적용할 수 있습니다.

### 구조예

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">편집하다</div>
  </c-pophover>
</b>
```

기본 레이아웃과 크기(18x18px), 호버 색상 변경이 `.Ico > c-pophover > a`에 통합되었습니다. 개발자는 다음과 같이 해당 아이콘 파일만 별도로 지정하면 됩니다.
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```