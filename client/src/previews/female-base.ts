import * as THREE from 'three'
import { disposeObjects } from './dispose-objects'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js'
import { RoomEnvironment } from 'three/examples/jsm/environments/RoomEnvironment.js'
import { femaleSeat, femaleSeatOffset } from './female-seat'
import { femaleUnderwearColor } from './female-underwear-color'
import { femaleTorchMotions, mirroredSwordGrip } from './female-torch'
import { FEMALE_HAND_FITS, poseOffHandProp } from '../lib/utils/handProps'
import { MODULAR_FEMALE_MODEL_PATH } from '../lib/utils/modelPaths'
import { bindModularPart, skinnedParts } from '../lib/utils/modularCharacter'
import { updateModularPoseCorrectives } from '../lib/utils/modularPoseCorrectives'
import { femaleOutfitMask } from '../lib/utils/female-outfit-mask'
import { femaleBracerMask } from './female-bracer-mask'
import {
  applySkinBrush,
  skinBrush,
  type SkinBrushEdits,
} from './female-skin-brush'
import {
  PART_FIT_KEYS,
  PART_FIT_RANGES,
  partFitter,
  SLEEVE_END_KEYS,
  sleeveEndDeformer,
  type PartFit,
  type PartFitKey,
  type PartFitter,
  type PartSide,
  type SleeveEnd,
} from './female-part-fit'
import {
  applyFemaleBracerFit,
  type BracerFitGeometry,
} from './female-bracer-fit'
import { femaleBootGround } from '../lib/utils/female-boot-ground'
import { femaleBrushCursor } from './female-brush-cursor'
import {
  attachValkyrieWings,
  disposeValkyrieWings,
  updateValkyrieWings,
} from '../lib/effects/valkyrie-wings'
import {
  updatePeltPhysics,
  resetPeltPhysics,
  disposePeltPhysics,
} from '../lib/effects/pelt-rig'

export async function showFemaleBase(
  scene: THREE.Scene,
  resources: THREE.Object3D[],
  cleanup: (() => void)[],
  disposed: () => boolean
) {
  document.title = '여성 베이스 · 캐릭터 미리보기'
  document.querySelector('h1')!.textContent = '여성 베이스 미리보기'
  document.querySelector('header p:last-child')!.textContent =
    '여성 체형에 맞춘 의상과 헤어, 동작을 살펴보세요.'
  const status = document.getElementById('status')!
  const aside = document.querySelector('aside')!
  aside.replaceChildren(status)
  aside.insertAdjacentHTML(
    'beforeend',
    `
    <fieldset id="female-controls" disabled>
      <legend>모델 살펴보기</legend>
      <label for="female-hair">헤어</label>
      <select id="female-hair">
        <option value="bob">금발 단발</option>
        <option value="none">없음</option>
        <option value="warrior">전사 긴 헤어 (피팅 검수 중)</option>
        <option value="ranger">순찰자 갈색 포니테일 (피팅 검수 중)</option>
      </select>
      <div id="warrior-hair-fit" hidden>
        <p class="details">처음 피팅한 헤어를 기준으로 조절합니다. 변경값은 미리보기에만 적용됩니다.</p>
        <label for="warrior-hair-scale">전체 크기 <output id="warrior-hair-scale-value">115%</output></label>
        <input id="warrior-hair-scale" type="range" min="80" max="160" step="1" value="115" />
        <label for="warrior-hair-height">높이 <output id="warrior-hair-height-value">118%</output></label>
        <input id="warrior-hair-height" type="range" min="70" max="150" step="1" value="118" />
        <label for="warrior-hair-offset">위아래 위치 <output id="warrior-hair-offset-value">-5.1cm</output></label>
        <input id="warrior-hair-offset" type="range" min="-10" max="10" step="0.1" value="-5.1" />
        <button id="warrior-hair-reset" type="button">헤어 조정 초기화</button>
      </div>
      <label class="toggle"><input id="female-wings" type="checkbox" /> 발키리 날개 (피팅 시안)</label>
      <a href="?asset=female-knight-bob">단발 원본 단독 보기</a>
      <label for="female-helmet">투구</label>
      <select id="female-helmet">
        <option value="none">없음</option>
        <option value="knight">은빛 기사 투구</option>
        <option value="valkyrie">발키리 날개 투구 (피팅 검수 중)</option>
        <option value="barbarian">야만전사 뿔 투구</option>
        <option value="brilliance">광휘의 투구</option>
        <option value="priest">사제관 (피팅 검수 중)</option>
      </select>
      <p class="details">뿔 투구 아래로 보이는 헤어는 남깁니다. 발키리 투구는 헤어와 함께 표시합니다. 전사 긴 헤어는 다른 투구에서 숨기고, 투구를 벗으면 복원합니다.</p>
      <label for="female-top">상의</label>
      <select id="female-top">
        <option value="none">없음</option>
        <option value="knight">은빛 기사 갑옷</option>
        <option value="barbarian">야만전사 가죽 철갑</option>
        <option value="valkyrie">발키리 상의 (피팅 검수 중)</option>
        <option value="caveman">원시전사 상의 (피팅 검수 중)</option>
        <option value="priest">사제 상의 (피팅 검수 중)</option>
        <option value="ranger">순찰자 조끼와 셔츠 (피팅 검수 중)</option>
        <option value="bard">바드 코르셋과 망토 (피팅 검수 중)</option>
        <option value="rogue">로그 가죽 코르셋</option>
      </select>
      <label for="female-bottom">하의</label>
      <select id="female-bottom">
        <option value="none">없음</option>
        <option value="knight">은빛 기사 골반 갑옷</option>
        <option value="barbarian">야만전사 모피 허리치마</option>
        <option value="valkyrie">발키리 하의 (피팅 검수 중)</option>
        <option value="caveman">원시전사 모피 하의 (피팅 검수 중)</option>
        <option value="priest">사제 치마 (피팅 검수 중)</option>
        <option value="ranger">순찰자 가죽 치마 (피팅 검수 중)</option>
        <option value="bard">바드 주름치마 (피팅 검수 중)</option>
        <option value="rogue">로그 벨트 미니스커트</option>
      </select>
      <label for="female-boots">신발</label>
      <select id="female-boots">
        <option value="none">없음</option>
        <option value="knight">은빛 기사 판금 부츠</option>
        <option value="caveman">원시전사 모피 하이힐 부츠 (피팅 검수 중)</option>
        <option value="priest">사제 하이힐 장화 (피팅 검수 중)</option>
        <option value="ranger">순찰자 스틸레토 부츠 (피팅 검수 중)</option>
        <option value="bard">바드 스틸레토 부츠 (피팅 검수 중)</option>
        <option value="rogue">로그 빨간 밑창 가죽 부츠</option>
        <option value="barbarian">야만전사 가죽 하이힐 샌들</option>
        <option value="valkyrie">발키리 부츠 (초기 피팅 검수 중)</option>
      </select>
      <label class="toggle"><input id="side-feet" type="checkbox" checked /> 발 옆모습 확대 (1cm 눈금)</label>
      <p id="sole-gap" class="details"></p>
      <label for="female-gloves">장갑</label>
      <select id="female-gloves">
        <option value="none">없음</option>
        <option value="knight">은빛 기사 판금 장갑</option>
        <option value="barbarian">야만전사 손목 보호대</option>
        <option value="valkyrie">발키리 손목 보호대 (초기 피팅)</option>
        <option value="caveman">원시전사 가죽 손목 보호대 (피팅 검수 중)</option>
        <option value="priest">사제 장갑 (피팅 검수 중)</option>
        <option value="ranger">순찰자 반장갑 (피팅 검수 중)</option>
        <option value="bard">바드 반장갑과 팔 보호대 (피팅 검수 중)</option>
        <option value="rogue">로그 검은 가죽 반장갑</option>
      </select>
      <div id="outfit-fit" hidden>
        <p><strong>복장 피팅 조절</strong></p>
        <label for="outfit-fit-part">조절할 부위</label>
        <select id="outfit-fit-part"></select>
        <div id="outfit-fit-side-row" hidden>
          <label for="outfit-fit-side">조절할 쪽</label>
          <select id="outfit-fit-side">
            <option value="both">양쪽 함께 (대칭)</option>
            <option value="left">캐릭터 왼쪽만</option>
            <option value="right">캐릭터 오른쪽만</option>
          </select>
          <p class="details">정면에서 볼 때 캐릭터 왼쪽은 화면 오른쪽입니다. 양쪽 함께에서 움직인 값은 두 쪽에 같게 들어갑니다. 좌우 이동은 바깥쪽이 +입니다.</p>
        </div>
        <label for="outfit-fit-move-x">좌우 이동 <output id="outfit-fit-move-x-value"></output> cm</label>
        <input id="outfit-fit-move-x" type="range" min="-10" max="10" step="0.1" />
        <label for="outfit-fit-move-y">위아래 이동 <output id="outfit-fit-move-y-value"></output> cm</label>
        <input id="outfit-fit-move-y" type="range" min="-10" max="10" step="0.1" />
        <label for="outfit-fit-move-z">앞뒤 이동 <output id="outfit-fit-move-z-value"></output> cm</label>
        <input id="outfit-fit-move-z" type="range" min="-10" max="10" step="0.1" />
        <label for="outfit-fit-scale">전체 크기 <output id="outfit-fit-scale-value"></output>%</label>
        <input id="outfit-fit-scale" type="range" min="70" max="130" step="0.5" />
        <label for="outfit-fit-scale-x">너비 <output id="outfit-fit-scale-x-value"></output>%</label>
        <input id="outfit-fit-scale-x" type="range" min="70" max="130" step="0.5" />
        <label for="outfit-fit-scale-y">높이 <output id="outfit-fit-scale-y-value"></output>%</label>
        <input id="outfit-fit-scale-y" type="range" min="70" max="130" step="0.5" />
        <label for="outfit-fit-scale-z">두께 <output id="outfit-fit-scale-z-value"></output>%</label>
        <input id="outfit-fit-scale-z" type="range" min="70" max="130" step="0.5" />
        <label for="outfit-fit-rotate-x">앞뒤 기울기 <output id="outfit-fit-rotate-x-value"></output>°</label>
        <input id="outfit-fit-rotate-x" type="range" min="-45" max="45" step="0.5" />
        <label for="outfit-fit-rotate-y">좌우 회전 <output id="outfit-fit-rotate-y-value"></output>°</label>
        <input id="outfit-fit-rotate-y" type="range" min="-45" max="45" step="0.5" />
        <label for="outfit-fit-rotate-z">옆 기울기 <output id="outfit-fit-rotate-z-value"></output>°</label>
        <input id="outfit-fit-rotate-z" type="range" min="-45" max="45" step="0.5" />
        <div id="sleeve-end-fit" hidden>
          <p class="details">소매 끝점을 상완 축 기준으로 옮깁니다. 어깨에서 끝까지 선형으로 따라갑니다.</p>
          <label for="sleeve-end-left-forward">왼쪽 소매 끝 앞뒤 <output id="sleeve-end-left-forward-value"></output> cm</label>
          <input id="sleeve-end-left-forward" type="range" min="-5" max="5" step="0.1" />
          <label for="sleeve-end-left-out">왼쪽 소매 끝 좌우 (바깥 +) <output id="sleeve-end-left-out-value"></output> cm</label>
          <input id="sleeve-end-left-out" type="range" min="-5" max="5" step="0.1" />
          <label for="sleeve-end-right-forward">오른쪽 소매 끝 앞뒤 <output id="sleeve-end-right-forward-value"></output> cm</label>
          <input id="sleeve-end-right-forward" type="range" min="-5" max="5" step="0.1" />
          <label for="sleeve-end-right-out">오른쪽 소매 끝 좌우 (바깥 +) <output id="sleeve-end-right-out-value"></output> cm</label>
          <input id="sleeve-end-right-out" type="range" min="-5" max="5" step="0.1" />
        </div>
        <label class="toggle"><input id="outfit-fit-show-skin" type="checkbox" /> 이 부위가 가린 피부 보이기 (맞춘 뒤 끄기)</label>
        <label class="toggle"><input id="outfit-brush" type="checkbox" /> 피부 브러시 (화면 클릭으로 칠하기)</label>
        <div id="outfit-brush-controls" hidden>
          <div id="outfit-brush-mode" class="segmented" role="radiogroup" aria-label="브러시">
            <label><input type="radio" name="outfit-brush-mode" value="hide" checked /> 가리기</label>
            <label><input type="radio" name="outfit-brush-mode" value="show" /> 되살리기</label>
          </div>
          <label for="outfit-brush-radius">반경 <output id="outfit-brush-radius-value">1.5</output> cm</label>
          <input id="outfit-brush-radius" type="range" min="0.3" max="5" step="0.1" value="1.5" />
          <button id="outfit-brush-undo" type="button">한 번 되돌리기</button>
          <button id="outfit-brush-clear" type="button">이 부위 브러시 지우기</button>
          <p class="details">드래그는 회전, 짧은 클릭은 칠하기입니다. 옷 뒤의 피부도 칠해집니다. 되살릴 곳은 '가린 피부 보이기'를 켜고 칠하세요. '이 부위 피팅 저장'으로 함께 저장됩니다.</p>
        </div>
        <button id="outfit-fit-save" type="button">이 부위 피팅 저장</button>
        <button id="outfit-fit-reset" type="button">이 부위 조절 초기화</button>
        <p id="outfit-fit-status" class="details" aria-live="polite"></p>
      </div>
      <div id="valkyrie-bracer-fit" hidden>
        <p><strong>발키리 손목 보호대</strong></p>
        <a id="bracer-edit-link">추가 피팅 조절</a>
        <label class="toggle"><input id="bracer-hide-skin" type="checkbox" checked /> 보호대 아래 팔 피부 가리기</label>
        <div id="bracer-fit-editor" hidden>
        <button id="bracer-save" type="button">현재 피팅 저장</button>
        <button id="bracer-load" type="button">저장한 피팅 불러오기</button>
        <p id="bracer-save-status" class="details" aria-live="polite"></p>
        <label for="bracer-move-arm-outward">팔에 수직 · 바깥쪽 축 이동 <output id="bracer-move-arm-outward-value">0</output> cm</label>
        <input id="bracer-move-arm-outward" type="range" min="-15" max="15" step="0.1" value="0" />
        <label for="bracer-move-arm-across">팔에 수직 · 몸과 평행한 축 이동 <output id="bracer-move-arm-across-value">0</output> cm</label>
        <input id="bracer-move-arm-across" type="range" min="-15" max="15" step="0.1" value="0" />
        <label for="bracer-move-arm-length">팔 방향 축 이동 <output id="bracer-move-arm-length-value">0</output> cm</label>
        <input id="bracer-move-arm-length" type="range" min="-15" max="15" step="0.1" value="0" />
        <label for="bracer-outward">좌우 바깥쪽 이동 <output id="bracer-outward-value">0</output> cm</label>
        <input id="bracer-outward" type="range" min="-10" max="10" step="0.1" value="0" />
        <label for="bracer-height">위아래 이동 <output id="bracer-height-value">0</output> cm</label>
        <input id="bracer-height" type="range" min="-15" max="15" step="0.1" value="0" />
        <label for="bracer-depth">앞뒤 이동 <output id="bracer-depth-value">0</output> cm</label>
        <input id="bracer-depth" type="range" min="-10" max="10" step="0.1" value="0" />
        <label for="bracer-scale">크기 <output id="bracer-scale-value">100</output>%</label>
        <input id="bracer-scale" type="range" min="50" max="180" step="1" value="100" />
        <label for="bracer-arm-outward">팔에 수직 · 바깥쪽 축 회전 <output id="bracer-arm-outward-value">0</output>°</label>
        <input id="bracer-arm-outward" type="range" min="-180" max="180" step="1" value="0" />
        <label for="bracer-arm-across">팔에 수직 · 몸과 평행한 축 회전 <output id="bracer-arm-across-value">0</output>°</label>
        <input id="bracer-arm-across" type="range" min="-180" max="180" step="1" value="0" />
        <label for="bracer-arm-twist">팔 방향 축 회전 <output id="bracer-arm-twist-value">0</output>°</label>
        <input id="bracer-arm-twist" type="range" min="-180" max="180" step="1" value="0" />
        <label for="bracer-rotate-x">X축 회전 <output id="bracer-rotate-x-value">0</output>°</label>
        <input id="bracer-rotate-x" type="range" min="-180" max="180" step="1" value="0" />
        <label for="bracer-rotate-y">Y축 회전 <output id="bracer-rotate-y-value">0</output>°</label>
        <input id="bracer-rotate-y" type="range" min="-180" max="180" step="1" value="0" />
        <label for="bracer-rotate-z">Z축 회전 <output id="bracer-rotate-z-value">0</output>°</label>
        <input id="bracer-rotate-z" type="range" min="-180" max="180" step="1" value="0" />
        <button id="bracer-reset" type="button">보호대 전체 조절 초기화</button>
        <p><strong>왼쪽만 추가 보정</strong></p>
        <label for="bracer-left-move-outward">바깥쪽 축 이동 <output id="bracer-left-move-outward-value">0</output> cm</label>
        <input id="bracer-left-move-outward" type="range" min="-10" max="10" step="0.1" value="0" />
        <label for="bracer-left-move-across">몸과 평행한 축 이동 <output id="bracer-left-move-across-value">0</output> cm</label>
        <input id="bracer-left-move-across" type="range" min="-10" max="10" step="0.1" value="0" />
        <label for="bracer-left-move-length">팔 방향 축 이동 <output id="bracer-left-move-length-value">0</output> cm</label>
        <input id="bracer-left-move-length" type="range" min="-10" max="10" step="0.1" value="0" />
        <label for="bracer-left-rotate-outward">바깥쪽 축 회전 <output id="bracer-left-rotate-outward-value">0</output>°</label>
        <input id="bracer-left-rotate-outward" type="range" min="-90" max="90" step="1" value="0" />
        <label for="bracer-left-rotate-across">몸과 평행한 축 회전 <output id="bracer-left-rotate-across-value">0</output>°</label>
        <input id="bracer-left-rotate-across" type="range" min="-90" max="90" step="1" value="0" />
        <label for="bracer-left-rotate-length">팔 방향 축 회전 <output id="bracer-left-rotate-length-value">0</output>°</label>
        <input id="bracer-left-rotate-length" type="range" min="-90" max="90" step="1" value="0" />
        <button id="bracer-left-reset" type="button">왼쪽 추가 보정만 초기화</button>
        <p class="details">게이지 위에서 마우스 휠을 돌려 한 단계씩 조절할 수 있습니다. 이동은 1mm 단위입니다. 공통 조절은 양쪽에 대칭으로 적용되고 왼쪽 추가 보정은 왼쪽에만 적용됩니다. 조절값은 이 브라우저에 저장됩니다. 조절값은 미리보기에서만 적용됩니다.</p>
        </div>
      </div>
      <label for="female-motion">동작</label>
      <select id="female-motion">
        <option value="rest">기본 A 포즈</option>
        <option value="idle1">대기</option>
        <option value="walk">걷기</option>
        <option value="run">달리기</option>
        <option value="jump">점프</option>
        <option value="combat_idle">전투 대기</option>
        <option value="slash1">베기</option>
        <option value="dying">쓰러지기</option>
        <option value="sit_idle">앉기</option>
      </select>
      <label for="female-equipment">장착 상태</label>
      <select id="female-equipment">
        <option value="unarmed">맨손</option>
        <option value="armed">강철 장검</option>
        <option value="torch">왼손 횃불</option>
        <option value="armed-torch">강철 장검 + 횃불</option>
        <option value="shield">왼손 나무 방패</option>
        <option value="armed-shield">강철 장검 + 나무 방패</option>
      </select>
      <p class="details">대기와 걷기에서 장착 상태에 따라 손 모양이 바뀝니다.</p>
      <p class="details">횃불을 장착하면 대기·걷기·달리기·전투 대기에서 왼손에 횃불을 듭니다.</p>
      <button id="female-pause" type="button">일시정지</button>
      <label for="female-time">동작 시점</label>
      <input id="female-time" type="range" min="0" max="1" step="0.001" value="0" />
      <label for="female-camera">시점</label>
      <select id="female-camera">
        <option value="face">얼굴 확대</option>
        <option value="upper">상의 확대</option>
        <option value="lower">하의 확대</option>
        <option value="lower-back">하의 후면 확대</option>
        <option value="full">전신 정면</option>
        <option value="side">전신 측면</option>
        <option value="back">전신 후면</option>
        <option value="feet">발 확대</option>
        <option value="boots">부츠 확대</option>
        <option value="boots-back">부츠 후면</option>
        <option value="boots-side">부츠 측면</option>
        <option value="right-hand">오른손 확대</option>
        <option value="left-hand">왼손 확대</option>
      </select>
      <label class="toggle"><input id="female-wireframe" type="checkbox" /> 와이어프레임</label>
      <label class="toggle"><input id="female-skeleton" type="checkbox" /> 뼈대 보기</label>
      <button id="female-reset" type="button">시점 초기화</button>
    </fieldset>
    <p id="female-stats" class="details"></p>
    <p class="details">투구·상의·하의·장갑·신발·헤어를 바꿔 착용 상태와 동작을 확인할 수 있습니다.</p>`
  )
  status.textContent = '미리보기 에셋을 준비하는 중…'
  const loader = new GLTFLoader(new THREE.LoadingManager())
  const hairSelect = document.getElementById('female-hair') as HTMLSelectElement
  const requestedHair = new URLSearchParams(location.search).get('hair')
  hairSelect.value =
    requestedHair &&
    ['bob', 'warrior', 'ranger', 'none'].includes(requestedHair)
      ? requestedHair
      : 'bob'
  const initialHair = hairSelect.value
  // Outfit hair is fitted with its outfit; the rest are prepared by the female preview plugin.
  const hairUrl = (name: string) =>
    name === 'ranger'
      ? '/__modular-character/parts/female_ranger_hair.glb'
      : `/__female-preview/${name}.glb`
  loader.manager.onProgress = (_url, loaded, total) => {
    if (!disposed())
      status.textContent = `미리보기 에셋을 불러오는 중… (${loaded}/${total})`
  }
  const [
    base,
    { scene: weapon },
    { scene: chair },
    { scene: torch },
    { scene: shield },
    prepared,
    initialHairAsset,
  ] = await Promise.all([
    loader.loadAsync('/__female-preview/base.glb'),
    loader.loadAsync('/models/weapons/steel_longsword.glb'),
    loader.loadAsync(`/models/${femaleSeat.chair.model}`),
    loader.loadAsync('/models/weapons/torch.glb'),
    loader.loadAsync('/models/armor/shield_wooden.glb'),
    fetch('/__female-preview/animations.json').then(async (response) => {
      if (!response.ok) throw new Error('여성 동작을 불러오지 못했습니다.')
      return (await response.json()) as {
        clips: ReturnType<typeof THREE.AnimationClip.toJSON>[]
      }
    }),
    initialHair === 'none' ? null : loader.loadAsync(hairUrl(initialHair)),
  ])
  const { scene: model, animations } = base
  loader.manager.onProgress = () => {}
  for (const data of prepared.clips) {
    const clip = THREE.AnimationClip.parse(data)
    const index = animations.findIndex((entry) => entry.name === clip.name)
    if (index === -1) animations.push(clip)
    else animations[index] = clip
  }
  const hairOptions: Record<string, THREE.SkinnedMesh[]> = { none: [] }
  let helmetOutfit = 'none'
  const hairKey = (name: string, helmet = helmetOutfit) =>
    (name === 'warrior' || name === 'ranger') &&
    helmet !== 'none' &&
    helmet !== 'valkyrie'
      ? 'none'
      : helmet === 'barbarian' && name !== 'none'
        ? `barbarian-${name}`
        : name
  if (initialHairAsset)
    hairOptions[initialHair] = bindModularPart(model, initialHairAsset.scene)
  const hairFitControls = document.getElementById('warrior-hair-fit')!
  const hairScale = document.getElementById(
    'warrior-hair-scale'
  ) as HTMLInputElement
  const hairHeight = document.getElementById(
    'warrior-hair-height'
  ) as HTMLInputElement
  const hairOffset = document.getElementById(
    'warrior-hair-offset'
  ) as HTMLInputElement
  const hairFitOriginals = new WeakMap<
    THREE.SkinnedMesh,
    {
      position: Float32Array
      normal?: Float32Array
      tangent?: Float32Array
    }
  >()
  const readAttribute = (
    attribute: THREE.BufferAttribute | THREE.InterleavedBufferAttribute
  ) => {
    const values = new Float32Array(attribute.count * attribute.itemSize)
    for (let i = 0; i < attribute.count; i++) {
      values[i * attribute.itemSize] = attribute.getX(i)
      values[i * attribute.itemSize + 1] = attribute.getY(i)
      values[i * attribute.itemSize + 2] = attribute.getZ(i)
      if (attribute.itemSize === 4)
        values[i * attribute.itemSize + 3] = attribute.getW(i)
    }
    return values
  }
  const updateHairFit = () => {
    const scale = Number(hairScale.value) / 115
    const vertical = (scale * Number(hairHeight.value)) / 118
    const offset = Number(hairOffset.value) / 100
    document.getElementById('warrior-hair-scale-value')!.textContent =
      `${hairScale.value}%`
    document.getElementById('warrior-hair-height-value')!.textContent =
      `${hairHeight.value}%`
    document.getElementById('warrior-hair-offset-value')!.textContent =
      `${Number(hairOffset.value).toFixed(1)}cm`
    for (const mesh of hairOptions.warrior ?? []) {
      const { position, normal, tangent } = mesh.geometry.attributes
      let original = hairFitOriginals.get(mesh)
      if (!original) {
        original = {
          position: readAttribute(position),
          normal: normal && readAttribute(normal),
          tangent: tangent && readAttribute(tangent),
        }
        hairFitOriginals.set(mesh, original)
      }
      for (let i = 0; i < position.count; i++) {
        const j = i * 3
        position.setXYZ(
          i,
          original.position[j] * scale,
          1.69 + (original.position[j + 1] - 1.639) * vertical + offset,
          -0.012 + (original.position[j + 2] + 0.012) * scale
        )
        if (normal && original.normal) {
          const x = original.normal[j] / scale
          const y = original.normal[j + 1] / vertical
          const z = original.normal[j + 2] / scale
          const length = Math.hypot(x, y, z) || 1
          normal.setXYZ(i, x / length, y / length, z / length)
        }
        if (tangent && original.tangent) {
          const k = i * 4
          const x = original.tangent[k] * scale
          const y = original.tangent[k + 1] * vertical
          const z = original.tangent[k + 2] * scale
          const length = Math.hypot(x, y, z) || 1
          tangent.setXYZW(
            i,
            x / length,
            y / length,
            z / length,
            original.tangent[k + 3]
          )
        }
      }
      position.needsUpdate = true
      if (normal) normal.needsUpdate = true
      if (tangent) tangent.needsUpdate = true
      mesh.geometry.computeBoundingSphere()
      mesh.frustumCulled = false
    }
  }
  hairScale.oninput = hairHeight.oninput = hairOffset.oninput = updateHairFit
  document.getElementById('warrior-hair-reset')!.onclick = () => {
    hairScale.value = '115'
    hairHeight.value = '118'
    hairOffset.value = '-5.1'
    updateHairFit()
  }
  hairFitControls.hidden = initialHair !== 'warrior'
  updateHairFit()
  const pendingHair = new Map<string, Promise<THREE.SkinnedMesh[]>>()
  const loadHair = (
    name: string,
    helmet = helmetOutfit
  ): Promise<THREE.SkinnedMesh[]> => {
    const key = hairKey(name, helmet)
    if (hairOptions[key]) return Promise.resolve(hairOptions[key])
    const pending = pendingHair.get(key)
    if (pending) return pending
    const promise = loader
      .loadAsync(
        key === name
          ? hairUrl(name)
          : `/__modular-character/parts/female_barbarian_helmet_hair_${name}.glb`
      )
      .then(({ scene: source }) => {
        if (disposed()) {
          disposeObjects([source])
          return []
        }
        const meshes = bindModularPart(model, source)
        for (const mesh of meshes) mesh.visible = false
        hairOptions[key] = meshes
        if (key === 'warrior') updateHairFit()
        return meshes
      })
      .finally(() => pendingHair.delete(key))
    pendingHair.set(key, promise)
    return promise
  }
  let helmetEquipped = false
  const updateHair = () => {
    for (const [name, meshes] of Object.entries(hairOptions))
      for (const mesh of meshes)
        mesh.visible =
          (!helmetEquipped ||
            helmetOutfit === 'barbarian' ||
            helmetOutfit === 'valkyrie' ||
            helmetOutfit === 'priest') &&
          hairKey(hairSelect.value) === name
  }
  updateHair()
  const swordGrip = animations.find((clip) => clip.name === 'idle_armed')
  const catwalk = animations.find((clip) => clip.name === 'walk')
  const combatIdle = animations.find((clip) => clip.name === 'combat_idle')
  if (!swordGrip || !catwalk || !combatIdle)
    throw new Error('검을 쥐는 손 자세 또는 걷기 동작을 찾지 못했습니다.')
  chair.visible = false
  resources.push(chair)
  const hand = model.getObjectByName('RightHand')
  if (!hand) throw new Error('검을 장착할 오른손 본을 찾지 못했습니다.')
  weapon.name = 'femaleSteelLongsword'
  weapon.position.copy(FEMALE_HAND_FITS['weapons/steel_longsword.glb'].position)
  weapon.rotation.copy(
    FEMALE_HAND_FITS['weapons/steel_longsword.glb'].rotation!
  )
  hand.add(weapon)
  const leftHand = model.getObjectByName('LeftHand')
  if (!leftHand) throw new Error('횃불을 장착할 왼손 본을 찾지 못했습니다.')
  torch.name = 'femaleTorch'
  torch.visible = false
  poseOffHandProp(torch, 'weapons/torch.glb', MODULAR_FEMALE_MODEL_PATH)
  leftHand.add(torch)
  shield.name = 'femaleWoodenShield'
  shield.visible = false
  poseOffHandProp(shield, 'armor/shield_wooden.glb', MODULAR_FEMALE_MODEL_PATH)
  leftHand.add(shield)
  const shieldGrip = mirroredSwordGrip(model, swordGrip).map((track) => ({
    bone: model.getObjectByName(track.name.split('.')[0])!,
    rotation: new THREE.Quaternion().fromArray(track.values),
  }))
  resources.push(model)
  if (disposed()) {
    disposeObjects([model, chair])
    return
  }
  scene.add(model)
  scene.add(chair)
  const bones: THREE.Bone[] = []
  model.traverse((node) => {
    if (node instanceof THREE.Bone) bones.push(node)
  })
  const rest = bones.map((bone) => ({
    bone,
    position: bone.position.clone(),
    quaternion: bone.quaternion.clone(),
    scale: bone.scale.clone(),
  }))
  const mixer = new THREE.AnimationMixer(model)
  let bootGround: ReturnType<typeof femaleBootGround> | undefined
  // The game lifts every heel by the plate boots' baked track, so the preview does too.
  let plateGround: ReturnType<typeof femaleBootGround> | undefined
  let loadPlateBoots = async () => {}
  const soleGap = document.getElementById('sole-gap')!
  const soleVertex = new THREE.Vector3()
  let gapMotion = ''
  let gapMax = 0
  let gapShownAt = 0
  const updateSoleGap = (now: number) => {
    const boots = bootsEquipped
      ? skinnedParts(model).filter(
          (mesh) => mesh.visible && /^boots_/.test(mesh.name)
        )
      : []
    if (!boots.length) {
      soleGap.textContent = ''
      return
    }
    let lowest = Infinity
    for (const mesh of boots) {
      const count = mesh.geometry.attributes.position.count
      for (let i = 0; i < count; i++) {
        mesh.getVertexPosition(i, soleVertex).applyMatrix4(mesh.matrixWorld)
        lowest = Math.min(lowest, soleVertex.y)
      }
    }
    if (gapMotion !== motion.value) {
      gapMotion = motion.value
      gapMax = 0
    }
    gapMax = Math.max(gapMax, lowest)
    if (now - gapShownAt < 200) return
    gapShownAt = now
    const mm = (metres: number) => Math.round(metres * 1000) || 0
    soleGap.textContent = `발바닥과 지면 간격: 지금 ${mm(lowest)}mm · 이 동작 최대 ${mm(gapMax)}mm`
  }
  let bootsEquipped = false
  let action: THREE.AnimationAction | undefined
  let paused = false
  const motion = document.getElementById('female-motion') as HTMLSelectElement
  const equipment = document.getElementById(
    'female-equipment'
  ) as HTMLSelectElement
  const pause = document.getElementById('female-pause') as HTMLButtonElement
  const time = document.getElementById('female-time') as HTMLInputElement
  const handPose = (clipName: string) => {
    const clip = animations.find((entry) => entry.name === clipName)
    if (!clip) throw new Error(`손 자세를 찾지 못했습니다: ${clipName}`)
    return clip.tracks
      .filter((track) =>
        /^RightHand(Thumb|Index|Middle|Ring|Pinky)[123]\.quaternion$/.test(
          track.name
        )
      )
      .map((track) => {
        const bone = model.getObjectByName(track.name.split('.')[0])
        if (!bone) throw new Error(`손가락 본을 찾지 못했습니다: ${track.name}`)
        return {
          bone,
          rotation: new THREE.Quaternion().fromArray(track.values),
        }
      })
  }
  const handPoses = {
    unarmed: handPose('idle1'),
    armed: handPose('idle_armed'),
  }
  const updateAnimation = (delta: number) => {
    bootGround?.restore()
    mixer.update(delta)
    updateModularPoseCorrectives(model, motion.value)
    if (shield.visible && motion.value !== 'rest') {
      for (const { bone, rotation } of shieldGrip)
        bone.quaternion.copy(rotation)
    }
    if (
      !['idle1', 'walk'].includes(motion.value) &&
      !(
        ['armed-torch', 'armed-shield'].includes(equipment.value) &&
        motion.value === 'run'
      )
    )
      return
    const pose = ['armed', 'armed-torch', 'armed-shield'].includes(
      equipment.value
    )
      ? handPoses.armed
      : handPoses.unarmed
    for (const { bone, rotation } of pose) bone.quaternion.copy(rotation)
  }
  const setPaused = (value: boolean) => {
    paused = value
    pause.textContent = paused ? '재생' : '일시정지'
  }
  const updateStatus = () => {
    status.textContent = ['idle1', 'walk', 'run', 'combat_idle'].includes(
      motion.value
    )
      ? `${motion.selectedOptions[0].textContent} · ${equipment.selectedOptions[0].textContent}`
      : motion.selectedOptions[0].textContent
  }
  const play = () => {
    bootGround?.restore()
    mixer.stopAllAction()
    chair.visible = motion.value === 'sit_idle'
    model.position.y = chair.visible ? femaleSeatOffset : 0
    for (const state of rest) {
      state.bone.position.copy(state.position)
      state.bone.quaternion.copy(state.quaternion)
      state.bone.scale.copy(state.scale)
    }
    weapon.visible = ['armed', 'armed-torch', 'armed-shield'].includes(
      equipment.value
    )
    shield.visible = ['shield', 'armed-shield'].includes(equipment.value)
    torch.visible =
      ['torch', 'armed-torch'].includes(equipment.value) &&
      !!femaleTorchMotions[motion.value]
    const clipName =
      torch.visible && weapon.visible && motion.value === 'idle1'
        ? 'torch_sword_idle1'
        : torch.visible
          ? femaleTorchMotions[motion.value]
          : motion.value
    const clip = animations.find((entry) => entry.name === clipName)
    action = clip ? mixer.clipAction(clip).reset().play() : undefined
    if (action) {
      action.setLoop(
        ['jump', 'slash1', 'dying'].includes(motion.value)
          ? THREE.LoopOnce
          : THREE.LoopRepeat,
        Infinity
      )
      action.clampWhenFinished = true
    }
    time.value = '0'
    time.disabled = pause.disabled = !action
    setPaused(false)
    updateAnimation(0)
    resetPeltPhysics(model)
    if (select.value === 'full' || select.value.endsWith('-hand')) reset()
    updateStatus()
    updateStats()
  }
  motion.onchange = play
  equipment.onchange = () => {
    const fraction = action ? action.time / action.getClip().duration : 0
    const wasPaused = paused
    play()
    if (action) {
      action.time = fraction * action.getClip().duration
      time.value = String(fraction)
      updateAnimation(0)
      setPaused(wasPaused)
    }
    if (select.value.endsWith('-hand')) reset()
  }
  pause.onclick = () => {
    if (action && action.time >= action.getClip().duration) {
      action.reset().play()
      time.value = '0'
      setPaused(false)
    } else {
      setPaused(!paused)
    }
  }
  mixer.addEventListener('finished', () => {
    setPaused(true)
    pause.textContent = '다시 재생'
  })
  time.oninput = () => {
    if (!action) return
    setPaused(true)
    action.paused = false
    action.time = Number(time.value) * action.getClip().duration
    updateAnimation(0)
    resetPeltPhysics(model)
    if (select.value.endsWith('-hand')) reset()
  }
  const skeleton = new THREE.SkeletonHelper(model)
  const skeletonToggle = document.getElementById(
    'female-skeleton'
  ) as HTMLInputElement
  skeleton.visible = skeletonToggle.checked =
    new URLSearchParams(location.search).get('skeleton') === '1'
  for (const material of Array.isArray(skeleton.material)
    ? skeleton.material
    : [skeleton.material]) {
    material.depthTest = false
  }
  skeleton.renderOrder = 10
  scene.add(skeleton)
  skeletonToggle.onchange = () => {
    skeleton.visible = skeletonToggle.checked
  }
  cleanup.push(() => {
    mixer.stopAllAction()
    mixer.uncacheRoot(model)
    scene.remove(skeleton)
    skeleton.dispose()
  })
  const materials = new Set<THREE.MeshStandardMaterial>()
  const collectMaterials = (root: THREE.Object3D) => {
    root.traverse((node) => {
      if (!(node instanceof THREE.Mesh)) return
      for (const material of Array.isArray(node.material)
        ? node.material
        : [node.material])
        if (material instanceof THREE.MeshStandardMaterial)
          materials.add(material)
    })
  }
  collectMaterials(model)
  let garters: THREE.SkinnedMesh[] = []
  let gartersPending: Promise<void> | undefined
  const gartersEquipped = () =>
    !!model.getObjectByName('bottom_female_rogue')?.visible &&
    !!model.getObjectByName('boots_female_rogue_right')?.visible
  const updateGarters = async () => {
    for (const mesh of garters) mesh.visible = gartersEquipped()
    if (!gartersEquipped()) return
    gartersPending ??= loader
      .loadAsync('/__modular-character/parts/female_rogue_garters.glb')
      .then((asset) => {
        if (disposed()) {
          disposeObjects([asset.scene])
          return
        }
        resources.push(asset.scene)
        garters = bindModularPart(model, asset.scene)
        for (const mesh of garters) collectMaterials(mesh)
        for (const material of materials) material.wireframe = wireframe.checked
      })
      .catch((error: unknown) => {
        gartersPending = undefined
        throw error
      })
    await gartersPending
    if (disposed()) return
    for (const mesh of garters) mesh.visible = gartersEquipped()
    updateModularPoseCorrectives(model, motion.value)
  }
  const underwearColor = femaleUnderwearColor(base)
  const outfitMask = femaleOutfitMask(base)
  let knightGlovesEquipped = false
  let sleeveMaskRegistered = false
  const updateGround = () => {
    bootGround?.restore()
    plateGround?.restore()
    model.position.y = chair.visible ? femaleSeatOffset : 0
    const ground = plateGround ?? bootGround
    if (bootsEquipped && ground) {
      const lift = ground.apply(
        chair.visible ? femaleSeatOffset : undefined,
        motion.value
      )
      if (!chair.visible) model.position.y = lift
    }
  }
  cleanup.push(() => outfitMask.dispose())
  cleanup.push(() => disposePeltPhysics(model))
  const bracerPanel = document.getElementById('valkyrie-bracer-fit')!
  const bracerEditing =
    new URLSearchParams(location.search).get('bracer-edit') === '1'
  document.getElementById('bracer-fit-editor')!.hidden = !bracerEditing
  const bracerEditLink = document.getElementById(
    'bracer-edit-link'
  ) as HTMLAnchorElement
  const bracerEditUrl = new URL(location.href)
  if (bracerEditing) bracerEditUrl.searchParams.delete('bracer-edit')
  else bracerEditUrl.searchParams.set('bracer-edit', '1')
  bracerEditLink.href = bracerEditUrl.href
  bracerEditLink.textContent = bracerEditing
    ? '저장된 피팅으로 보기'
    : '추가 피팅 조절'
  const bracerKeys = [
    'outward',
    'height',
    'depth',
    'scale',
    'rotate-x',
    'rotate-y',
    'rotate-z',
    'arm-outward',
    'arm-across',
    'arm-twist',
    'move-arm-outward',
    'move-arm-across',
    'move-arm-length',
    'left-move-outward',
    'left-move-across',
    'left-move-length',
    'left-rotate-outward',
    'left-rotate-across',
    'left-rotate-length',
  ] as const
  const bracerInputs = Object.fromEntries(
    bracerKeys.map((key) => [
      key,
      document.getElementById(`bracer-${key}`) as HTMLInputElement,
    ])
  ) as Record<(typeof bracerKeys)[number], HTMLInputElement>
  const bracerStorageKey = 'female-valkyrie-bracer-fit-v1'
  let hasStoredBracerFit = false
  let bracerSnapshotSent = false
  try {
    const saved: unknown = JSON.parse(
      localStorage.getItem(bracerStorageKey) ?? 'null'
    )
    if (saved && typeof saved === 'object') {
      hasStoredBracerFit =
        typeof (saved as Record<string, unknown>).scale === 'number'
      for (const key of bracerKeys) {
        const value = (saved as Record<string, unknown>)[key]
        if (typeof value === 'number' && Number.isFinite(value))
          bracerInputs[key].value = String(value)
      }
    }
  } catch {
    // Storage is optional.
  }
  let bracerAxes: THREE.Vector3[] = []
  let bracerSegments: [THREE.Vector3, THREE.Vector3][] = []
  let bracerBones: number[][] = []
  let bracerMaskTimer: ReturnType<typeof setTimeout> | undefined
  let bracerMaskBody: THREE.BufferGeometry | undefined
  let brushBody: THREE.Mesh | undefined
  base.scene.traverse((node) => {
    const association = base.parser.associations.get(node)
    if (
      node instanceof THREE.Mesh &&
      association?.meshes === 0 &&
      association.primitives === 0
    ) {
      bracerMaskBody = node.geometry
      brushBody = node
    }
  })
  // Masks rebuild the index but keep vertices, so the unmasked index maps strokes to mask triangle numbers.
  const brushRestIndex = bracerMaskBody?.index?.array.slice()
  const bracerHideSkin = document.getElementById(
    'bracer-hide-skin'
  ) as HTMLInputElement
  let bracerMaskEnabled = false
  let bracerStaticMask: unknown
  const refreshBracerMask = () => {
    if (!bracerMaskEnabled || !bracerMaskBody) return
    if (bracerStaticMask) {
      outfitMask.unregister('gloves-valkyrie')
      outfitMask.register('gloves-valkyrie', bracerStaticMask)
      outfitMask.setEquipped('gloves-valkyrie', true)
      return
    }
    if (!bracerGeometry.length) return
    const mask = femaleBracerMask(
      bracerMaskBody,
      bracerGeometry.map(({ mesh }) => mesh),
      bracerSegments,
      bracerBones
    )
    outfitMask.unregister('gloves-valkyrie')
    outfitMask.register('gloves-valkyrie', mask)
    outfitMask.setEquipped('gloves-valkyrie', true)
  }
  cleanup.push(() => clearTimeout(bracerMaskTimer))
  let bracerGeometry: BracerFitGeometry[] = []
  const updateBracerFit = () => {
    const values = Object.fromEntries(
      bracerKeys.map((key) => [key, Number(bracerInputs[key].value)])
    ) as Record<(typeof bracerKeys)[number], number>
    for (const key of bracerKeys)
      document.getElementById(`bracer-${key}-value`)!.textContent = String(
        values[key]
      )
    applyFemaleBracerFit(bracerGeometry, bracerAxes, values)
    clearTimeout(bracerMaskTimer)
    if (bracerMaskEnabled) bracerMaskTimer = setTimeout(refreshBracerMask, 250)
    try {
      localStorage.setItem(bracerStorageKey, JSON.stringify(values))
    } catch {
      // Storage is optional.
    }
  }
  const bracerSaveStatus = document.getElementById('bracer-save-status')!
  const saveBracerFit = async (automatic = false) => {
    const button = document.getElementById('bracer-save') as HTMLButtonElement
    button.disabled = true
    bracerSaveStatus.textContent = '피팅을 저장하는 중…'
    try {
      const response = await fetch('/__female-preview/valkyrie-fit.json', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          settings: Object.fromEntries(
            bracerKeys.map((key) => [key, Number(bracerInputs[key].value)])
          ),
          hide_skin: bracerHideSkin.checked,
          automatic,
        }),
      })
      const result = (await response.json()) as {
        saved?: boolean
        error?: string
      }
      if (!response.ok)
        throw new Error(result.error ?? '피팅을 저장하지 못했습니다.')
      bracerSaveStatus.textContent = result.saved
        ? '현재 피팅을 파일에 저장했습니다.'
        : '기존 저장본을 유지했습니다.'
    } catch (error) {
      bracerSaveStatus.textContent =
        error instanceof Error ? error.message : '피팅을 저장하지 못했습니다.'
    } finally {
      button.disabled = false
    }
  }
  if (import.meta.hot) {
    const captureBracerFit = () => {
      if (bracerEditing) void saveBracerFit()
    }
    import.meta.hot.on('valkyrie-save-fit', captureBracerFit)
    cleanup.push(() =>
      import.meta.hot?.off('valkyrie-save-fit', captureBracerFit)
    )
  }
  document.getElementById('bracer-save')!.onclick = () => {
    void saveBracerFit()
  }
  document.getElementById('bracer-load')!.onclick = async () => {
    try {
      const response = await fetch('/__female-preview/valkyrie-fit.json')
      const result = (await response.json()) as {
        settings?: Record<string, number>
        hide_skin?: boolean
        error?: string
      }
      if (!response.ok || !result.settings)
        throw new Error(result.error ?? '저장된 피팅을 불러오지 못했습니다.')
      for (const key of bracerKeys) {
        const value = result.settings[key]
        if (typeof value === 'number' && Number.isFinite(value))
          bracerInputs[key].value = String(value)
      }
      bracerHideSkin.checked = result.hide_skin === true
      updateBracerFit()
      bracerHideSkin.dispatchEvent(new Event('change'))
      bracerSaveStatus.textContent = '저장한 피팅을 불러왔습니다.'
    } catch (error) {
      bracerSaveStatus.textContent =
        error instanceof Error ? error.message : '피팅을 불러오지 못했습니다.'
    }
  }
  bracerHideSkin.onchange = () => {
    bracerMaskEnabled =
      bracerHideSkin.checked &&
      (document.getElementById('female-gloves') as HTMLSelectElement).value ===
        'valkyrie'
    clearTimeout(bracerMaskTimer)
    if (bracerMaskEnabled) refreshBracerMask()
    else outfitMask.unregister('gloves-valkyrie')
  }
  for (const key of bracerKeys) {
    const input = bracerInputs[key]
    input.oninput = updateBracerFit
    input.addEventListener(
      'wheel',
      (event) => {
        if (event.deltaY === 0) return
        event.preventDefault()
        if (event.deltaY < 0) input.stepUp()
        else input.stepDown()
        updateBracerFit()
      },
      { passive: false }
    )
  }
  document.getElementById('bracer-reset')!.onclick = () => {
    for (const key of bracerKeys)
      bracerInputs[key].value = key === 'scale' ? '100' : '0'
    updateBracerFit()
  }
  document.getElementById('bracer-left-reset')!.onclick = () => {
    for (const key of bracerKeys)
      if (key.startsWith('left-')) bracerInputs[key].value = '0'
    updateBracerFit()
  }
  updateBracerFit()
  const fitPanel = document.getElementById('outfit-fit')!
  const fitPart = document.getElementById(
    'outfit-fit-part'
  ) as HTMLSelectElement
  const fitStatus = document.getElementById('outfit-fit-status')!
  // Outfits whose parts are fitted by hand here; state is keyed by part (`${slot}-${outfit}`).
  const fittableOutfits: Record<string, string> = {
    priest: '사제',
    ranger: '순찰자',
    bard: '바드',
  }
  const fitSlotNames: Record<string, string> = {
    helmet: '투구',
    top: '상의',
    bottom: '치마',
    gloves: '장갑',
    boots: '부츠',
  }
  const partSlot = (part: string) => part.split('-')[0]
  const partOutfit = (part: string) => part.split('-')[1]
  const partName = (part: string) =>
    `${fittableOutfits[partOutfit(part)]} ${fitSlotNames[partSlot(part)]}`
  const fitters = new Map<string, PartFitter>()
  // Paired parts keep a fit per side; side 1 is the character's left (+x).
  const partFits = new Map<string, Record<PartSide, PartFit>>()
  const pairedSlots = ['gloves', 'boots']
  const fitSide = document.getElementById(
    'outfit-fit-side'
  ) as HTMLSelectElement
  const fitSideRow = document.getElementById('outfit-fit-side-row')!
  const defaultPartFit = () =>
    Object.fromEntries(
      PART_FIT_KEYS.map((key) => [key, PART_FIT_RANGES[key][2]])
    ) as Record<PartFitKey, number> as PartFit
  // New glove fits use the forearm frame; fits saved before it keep the character's axes.
  const newPartFit = (part: string): PartFit => ({
    ...defaultPartFit(),
    ...(partSlot(part) === 'gloves' && { frame: 'forearm' as const }),
  })
  const sidesOf = (part: string) => {
    let sides = partFits.get(part)
    if (!sides) {
      sides = { 1: newPartFit(part), [-1]: newPartFit(part) }
      partFits.set(part, sides)
    }
    return sides
  }
  const editedSides = (part: string): PartSide[] =>
    !pairedSlots.includes(partSlot(part)) || fitSide.value === 'both'
      ? [1, -1]
      : fitSide.value === 'left'
        ? [1]
        : [-1]
  const fitInputs = Object.fromEntries(
    PART_FIT_KEYS.map((key) => [
      key,
      document.getElementById(`outfit-fit-${key}`) as HTMLInputElement,
    ])
  ) as Record<PartFitKey, HTMLInputElement>
  // Parts whose hidden body skin is shown again for lining the part up against the body.
  const skinShown = new Set<string>()
  const showSkinToggle = document.getElementById(
    'outfit-fit-show-skin'
  ) as HTMLInputElement
  showSkinToggle.onchange = () => {
    const part = fitPart.value
    if (showSkinToggle.checked) skinShown.add(part)
    else skinShown.delete(part)
    if (fitters.has(part)) outfitMask.setEquipped(part, !showSkinToggle.checked)
  }
  // Parts whose sleeves are aligned as one tube, so their ends can be moved by hand.
  const sleeveEndParts = new Set(['top-ranger'])
  const sleeveEnds = new Map<string, Record<PartSide, SleeveEnd>>()
  const sleeveEndsOf = (part: string) => {
    let ends = sleeveEnds.get(part)
    if (!ends) {
      ends = { 1: { forward: 0, out: 0 }, [-1]: { forward: 0, out: 0 } }
      sleeveEnds.set(part, ends)
    }
    return ends
  }
  const sleeveEndInputs = ([1, -1] as const).flatMap((side) =>
    SLEEVE_END_KEYS.map((key) => {
      const id = `sleeve-end-${side === 1 ? 'left' : 'right'}-${key}`
      return {
        side,
        key,
        input: document.getElementById(id) as HTMLInputElement,
        output: document.getElementById(`${id}-value`)!,
      }
    })
  )
  const showFit = () => {
    const sleeveEnd = sleeveEndParts.has(fitPart.value)
    document.getElementById('sleeve-end-fit')!.hidden = !sleeveEnd
    if (sleeveEnd)
      for (const { side, key, input, output } of sleeveEndInputs)
        input.value = output.textContent = String(
          sleeveEndsOf(fitPart.value)[side][key]
        )
    fitSideRow.hidden = !pairedSlots.includes(partSlot(fitPart.value))
    showSkinToggle.checked = skinShown.has(fitPart.value)
    const fit = sidesOf(fitPart.value)[editedSides(fitPart.value)[0]]
    for (const key of PART_FIT_KEYS) {
      fitInputs[key].value = String(fit[key])
      document.getElementById(`outfit-fit-${key}-value`)!.textContent = String(
        fit[key]
      )
    }
  }
  type SavedFit = (PartFit | { left: PartFit; right: PartFit }) & {
    'sleeve-end'?: { left: SleeveEnd; right: SleeveEnd }
  }
  const brushEdits = new Map<string, SkinBrushEdits>()
  const fitMasks = new Map<string, { hide_triangle_indices: number[] }>()
  const brushOf = (part: string) => {
    let edits = brushEdits.get(part)
    if (!edits) {
      edits = { hide: new Set(), show: new Set() }
      brushEdits.set(part, edits)
    }
    return edits
  }
  // The part's baked mask with the brush strokes applied.
  const brushedMask = (part: string) => {
    const mask = fitMasks.get(part)!
    return {
      ...mask,
      hide_triangle_indices: applySkinBrush(
        mask.hide_triangle_indices,
        brushOf(part)
      ),
    }
  }
  // Re-registers a part's mask with its brush edits, keeping its equipped state.
  const applyFitMask = (part: string) => {
    if (!fitMasks.has(part)) return
    outfitMask.unregister(part)
    outfitMask.register(part, brushedMask(part))
    outfitMask.setEquipped(
      part,
      (document.getElementById(`female-${partSlot(part)}`) as HTMLSelectElement)
        .value === partOutfit(part) && !skinShown.has(part)
    )
  }
  const fitUrl = (outfit: string) => `/__female-preview/${outfit}-fit.json`
  const savedFits = Promise.all(
    Object.keys(fittableOutfits).map((outfit) =>
      fetch(fitUrl(outfit))
        .then((response) => (response.ok ? response.json() : { parts: {} }))
        .then(
          (saved: {
            parts: Record<
              string,
              { pending?: SavedFit; brush?: { hide: number[]; show: number[] } }
            >
          }) => {
            for (const [slot, entry] of Object.entries(saved.parts)) {
              const part = `${slot}-${outfit}`
              if (entry.brush && !brushEdits.has(part))
                brushEdits.set(part, {
                  hide: new Set(entry.brush.hide),
                  show: new Set(entry.brush.show),
                })
              const pending = entry.pending
              if (!pending || partFits.has(part)) continue
              const ends = pending['sleeve-end']
              if (ends) sleeveEnds.set(part, { 1: ends.left, [-1]: ends.right })
              const [left, right] =
                'left' in pending
                  ? [pending.left, pending.right]
                  : [pending, pending]
              partFits.set(part, {
                1: { ...defaultPartFit(), ...left },
                [-1]: { ...defaultPartFit(), ...right },
              })
            }
          }
        )
        .catch(() => undefined)
    )
  )
  const applyFit = (part: string) => {
    const sides = sidesOf(part)
    fitters.get(part)?.((side) => sides[side])
  }
  for (const key of PART_FIT_KEYS)
    fitInputs[key].oninput = () => {
      const sides = sidesOf(fitPart.value)
      for (const side of editedSides(fitPart.value))
        sides[side][key] = Number(fitInputs[key].value)
      showFit()
      applyFit(fitPart.value)
      fitStatus.textContent = '저장하지 않은 조절이 있습니다.'
    }
  // One wheel notch moves a slider by one step (1 mm for the move sliders).
  for (const input of [
    ...Object.values(fitInputs),
    ...sleeveEndInputs.map(({ input }) => input),
  ])
    input.addEventListener(
      'wheel',
      (event) => {
        event.preventDefault()
        if (event.deltaY === 0) return
        if (event.deltaY < 0) input.stepUp()
        else input.stepDown()
        input.dispatchEvent(new Event('input'))
      },
      { passive: false }
    )
  fitPart.onchange = fitSide.onchange = showFit
  for (const { side, key, input } of sleeveEndInputs) {
    input.oninput = () => {
      sleeveEndsOf(fitPart.value)[side][key] = Number(input.value)
      showFit()
      applyFit(fitPart.value)
      fitStatus.textContent = '저장하지 않은 조절이 있습니다.'
    }
  }
  document.getElementById('outfit-fit-reset')!.onclick = () => {
    const sides = sidesOf(fitPart.value)
    for (const side of editedSides(fitPart.value))
      sides[side] = newPartFit(fitPart.value)
    sleeveEnds.delete(fitPart.value)
    showFit()
    applyFit(fitPart.value)
  }
  document.getElementById('outfit-fit-save')!.onclick = async () => {
    const part = fitPart.value
    fitStatus.textContent = '저장하는 중…'
    try {
      const response = await fetch(fitUrl(partOutfit(part)), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          part: partSlot(part),
          settings: pairedSlots.includes(partSlot(part))
            ? { left: sidesOf(part)[1], right: sidesOf(part)[-1] }
            : {
                ...sidesOf(part)[1],
                ...(sleeveEndParts.has(part) && {
                  'sleeve-end': {
                    left: sleeveEndsOf(part)[1],
                    right: sleeveEndsOf(part)[-1],
                  },
                }),
              },
          brush: {
            hide: [...brushOf(part).hide],
            show: [...brushOf(part).show],
          },
        }),
      })
      const result = (await response.json()) as { error?: string }
      fitStatus.textContent = response.ok
        ? `${partName(part)} 피팅을 저장했습니다. 베이크하면 모델에 반영됩니다.`
        : (result.error ?? '저장하지 못했습니다.')
    } catch {
      fitStatus.textContent = '저장하지 못했습니다.'
    }
  }
  const brushToggle = document.getElementById(
    'outfit-brush'
  ) as HTMLInputElement
  const brushModeGroup = document.getElementById('outfit-brush-mode')!
  const brushMode = {
    get value() {
      return brushModeGroup.querySelector<HTMLInputElement>('input:checked')!
        .value
    },
  }
  const brushRadius = document.getElementById(
    'outfit-brush-radius'
  ) as HTMLInputElement
  const brushHistory: { part: string; hide: number[]; show: number[] }[] = []
  // Brushing holds the pose still; playback resumes afterwards only if the brush paused it.
  let brushPaused = false
  brushToggle.onchange = () => {
    document.getElementById('outfit-brush-controls')!.hidden =
      !brushToggle.checked
    if (brushToggle.checked && !paused) {
      brushPaused = true
      setPaused(true)
    } else if (!brushToggle.checked && brushPaused) {
      brushPaused = false
      setPaused(false)
    }
  }
  brushRadius.oninput = () => {
    document.getElementById('outfit-brush-radius-value')!.textContent =
      brushRadius.value
  }
  document.getElementById('outfit-brush-undo')!.onclick = () => {
    const last = brushHistory.pop()
    if (!last) return
    brushEdits.set(last.part, {
      hide: new Set(last.hide),
      show: new Set(last.show),
    })
    applyFitMask(last.part)
  }
  document.getElementById('outfit-brush-clear')!.onclick = () => {
    const part = fitPart.value
    const edits = brushOf(part)
    brushHistory.push({ part, hide: [...edits.hide], show: [...edits.show] })
    brushEdits.set(part, { hide: new Set(), show: new Set() })
    applyFitMask(part)
    fitStatus.textContent = '저장하지 않은 조절이 있습니다.'
  }
  function setupSkinBrush() {
    if (!brushBody || !brushRestIndex) return
    const body = brushBody
    const pick = skinBrush(
      body.geometry.getAttribute('position') as THREE.BufferAttribute,
      brushRestIndex
    )
    const raycaster = new THREE.Raycaster()
    let down: { x: number; y: number } | undefined
    const canvas = renderer.domElement
    const cursor = femaleBrushCursor(body)
    const position = body.geometry.getAttribute(
      'position'
    ) as THREE.BufferAttribute
    const restPoint = new THREE.Vector3()
    const corner = new THREE.Vector3()
    let hover: PointerEvent | undefined
    let hoverFrame = 0
    const updateCursor = () => {
      hoverFrame = 0
      const event = hover
      if (!event || !brushToggle.checked || fitPanel.hidden) {
        cursor.hide()
        return
      }
      const rect = canvas.getBoundingClientRect()
      raycaster.setFromCamera(
        new THREE.Vector2(
          ((event.clientX - rect.left) / rect.width) * 2 - 1,
          -((event.clientY - rect.top) / rect.height) * 2 + 1
        ),
        camera
      )
      if (body instanceof THREE.SkinnedMesh) body.computeBoundingSphere()
      const hit = raycaster.intersectObject(body)[0]
      if (!hit?.face || !hit.barycoord) {
        cursor.hide()
        return
      }
      restPoint.set(0, 0, 0)
      for (const [vertex, weight] of [
        [hit.face.a, hit.barycoord.x],
        [hit.face.b, hit.barycoord.y],
        [hit.face.c, hit.barycoord.z],
      ])
        restPoint.addScaledVector(
          corner.fromBufferAttribute(position, vertex),
          weight
        )
      cursor.show(
        restPoint,
        Number(brushRadius.value) / 100,
        brushMode.value === 'hide' ? '#ff5050' : '#50e080'
      )
    }
    const onMove = (event: PointerEvent) => {
      hover = event
      hoverFrame ||= requestAnimationFrame(updateCursor)
    }
    const onLeave = () => {
      hover = undefined
      cursor.hide()
    }
    for (const control of [brushToggle, brushModeGroup, brushRadius])
      control.addEventListener('input', () => {
        if (hover) onMove(hover)
        else cursor.hide()
      })
    const onDown = (event: PointerEvent) => {
      down = { x: event.clientX, y: event.clientY }
    }
    const onUp = (event: PointerEvent) => {
      const start = down
      down = undefined
      if (!start || !brushToggle.checked || fitPanel.hidden) return
      if (Math.hypot(event.clientX - start.x, event.clientY - start.y) > 4)
        return
      const rect = canvas.getBoundingClientRect()
      raycaster.setFromCamera(
        new THREE.Vector2(
          ((event.clientX - rect.left) / rect.width) * 2 - 1,
          -((event.clientY - rect.top) / rect.height) * 2 + 1
        ),
        camera
      )
      if (body instanceof THREE.SkinnedMesh) body.computeBoundingSphere()
      // Names what is in front at the click, since a speck that is not body skin cannot be brushed.
      for (const mesh of model.children.flatMap((child) =>
        child.getObjectsByProperty('isSkinnedMesh', true)
      ))
        (mesh as THREE.SkinnedMesh).computeBoundingSphere()
      const hits = raycaster.intersectObject(model, true)
      const front = hits.find(
        (candidate) =>
          candidate.object.visible &&
          (candidate.object as THREE.Mesh).material !== undefined
      )
      const hit = hits.find((candidate) => candidate.object === body)
      fitStatus.textContent = `클릭한 곳 맨 앞: ${front ? front.object.name || '(이름 없음)' : '없음'}${front?.object === body ? ' (몸 피부)' : ''}`
      if (!hit?.face || !hit.barycoord) return
      const part = fitPart.value
      const edits = brushOf(part)
      brushHistory.push({ part, hide: [...edits.hide], show: [...edits.show] })
      const [add, remove] =
        brushMode.value === 'hide'
          ? [edits.hide, edits.show]
          : [edits.show, edits.hide]
      for (const triangle of pick(
        hit.face,
        hit.barycoord,
        Number(brushRadius.value) / 100
      )) {
        add.add(triangle)
        remove.delete(triangle)
      }
      applyFitMask(part)
      fitStatus.textContent += ' · 저장하지 않은 조절이 있습니다.'
    }
    canvas.addEventListener('pointerdown', onDown)
    canvas.addEventListener('pointerup', onUp)
    canvas.addEventListener('pointermove', onMove)
    canvas.addEventListener('pointerleave', onLeave)
    cleanup.push(() => {
      cancelAnimationFrame(hoverFrame)
      canvas.removeEventListener('pointerdown', onDown)
      canvas.removeEventListener('pointerup', onUp)
      canvas.removeEventListener('pointermove', onMove)
      canvas.removeEventListener('pointerleave', onLeave)
    })
  }
  const updateFitPanel = () => {
    const equipped = Object.keys(fitSlotNames)
      .map(
        (slot) =>
          `${slot}-${(document.getElementById(`female-${slot}`) as HTMLSelectElement).value}`
      )
      .filter((part) => fitters.has(part))
    fitPanel.hidden = !equipped.length
    const current = fitPart.value
    fitPart.replaceChildren(
      ...equipped.map((part) => new Option(partName(part), part))
    )
    if (equipped.includes(current)) fitPart.value = current
    showFit()
  }
  const outfitSlots = ['top', 'bottom', 'boots', 'gloves', 'helmet']
  const outfitUpdates = outfitSlots.map((slot) => {
    const select = document.getElementById(
      `female-${slot}`
    ) as HTMLSelectElement
    const outfits = [...select.options]
      .map((option) => option.value)
      .filter((value) => value !== 'none')
    const requestedOutfit = new URLSearchParams(location.search).get(slot)
    select.value =
      requestedOutfit && outfits.includes(requestedOutfit)
        ? requestedOutfit
        : 'none'
    const loaded = new Map<
      string,
      {
        meshes?: THREE.SkinnedMesh[]
        pending?: Promise<void>
        ground?: ReturnType<typeof femaleBootGround>
      }
    >()
    const maskName = (outfit: string) => `${slot}-${outfit}`
    const load = (outfit: string) => {
      const entry = loaded.get(outfit) ?? {}
      loaded.set(outfit, entry)
      if (!entry.pending) {
        let source: THREE.Object3D | undefined
        const sleeves = slot === 'top' && outfit === 'knight'
        const valkyrie = slot === 'boots' && outfit === 'valkyrie'
        const bracer = slot === 'gloves' && outfit === 'valkyrie'
        const bakedBracer = bracer && !bracerEditing
        entry.pending = (async () => {
          let mask: unknown
          if (!bracer || bakedBracer) {
            const response = await fetch(
              `/__modular-character/female-${outfit}-${slot}-occlusion.json`
            )
            if (!response.ok)
              throw new Error('의상 가림 정보를 불러오지 못했습니다.')
            mask = await response.json()
          }
          let sleeveMask: unknown
          if (sleeves) {
            const response = await fetch(
              '/__modular-character/female-knight-gloves-top-occlusion.json'
            )
            if (!response.ok)
              throw new Error('장갑과 소매의 가림 정보를 불러오지 못했습니다.')
            sleeveMask = await response.json()
          }
          const asset = await loader.loadAsync(
            valkyrie
              ? '/__female-preview/valkyrie.glb'
              : bracer && bracerEditing
                ? '/__modular-character/parts/female_valkyrie_gloves_source.glb'
                : `/__modular-character/parts/female_${outfit}_${slot}.glb`
          )
          source = asset.scene
          if (disposed()) {
            disposeObjects([source])
            return
          }
          const fittable = outfit in fittableOutfits
          const part = maskName(outfit)
          if (fittable) {
            await savedFits
            if (disposed()) return
            fitMasks.set(part, mask as { hide_triangle_indices: number[] })
            outfitMask.register(part, brushedMask(part))
          } else if (!bracer) outfitMask.register(part, mask)
          if (bracer) bracerStaticMask = bakedBracer ? mask : undefined
          const meshes = (entry.meshes = bindModularPart(model, source))
          const skeleton = meshes[0].skeleton
          const bindPosition = (name: string) => {
            const index = skeleton.bones.findIndex((bone) => bone.name === name)
            if (index < 0) throw new Error(`팔 본을 찾지 못했습니다: ${name}`)
            return new THREE.Vector3().setFromMatrixPosition(
              skeleton.boneInverses[index].clone().invert()
            )
          }
          if (fittable) {
            let after: ((point: THREE.Vector3) => void) | undefined
            if (sleeveEndParts.has(part)) {
              // Read before the fitter moves them: the unmoved positions fix each sleeve's end.
              const deform = sleeveEndDeformer(
                meshes.map(
                  (mesh) =>
                    mesh.geometry.getAttribute('position').array as Float32Array
                ),
                {
                  1: [bindPosition('LeftArm'), bindPosition('LeftForeArm')],
                  [-1]: [
                    bindPosition('RightArm'),
                    bindPosition('RightForeArm'),
                  ],
                }
              )
              after = (point) =>
                deform(point, (side) => sleeveEndsOf(part)[side])
            }
            fitters.set(
              part,
              partFitter(
                meshes,
                pairedSlots.includes(slot),
                after,
                slot === 'gloves'
                  ? {
                      1: [
                        bindPosition('LeftForeArm'),
                        bindPosition('LeftHand'),
                      ],
                      [-1]: [
                        bindPosition('RightForeArm'),
                        bindPosition('RightHand'),
                      ],
                    }
                  : undefined
              )
            )
            applyFit(part)
          }
          if (bracer && bracerEditing) {
            const arm = bindPosition('LeftHand')
              .sub(bindPosition('LeftForeArm'))
              .normalize()
            const outward = new THREE.Vector3(1, 0, 0)
              .addScaledVector(arm, -arm.x)
              .normalize()
            const across = new THREE.Vector3()
              .crossVectors(arm, outward)
              .normalize()
            bracerAxes = [outward, across, arm]
            if (!bracerMaskBody)
              throw new Error('보호대의 기준 몸체를 찾지 못했습니다.')
            const sides = meshes.map((mesh) => {
              mesh.geometry.computeBoundingBox()
              return mesh.geometry.boundingBox!.getCenter(new THREE.Vector3())
                .x < 0
                ? 'Right'
                : 'Left'
            })
            bracerSegments = sides.map((side) => [
              bindPosition(`${side}ForeArm`),
              bindPosition(`${side}Hand`),
            ])
            bracerBones = sides.map((side) =>
              [`${side}Arm`, `${side}ForeArm`, `${side}Hand`].map((name) =>
                skeleton.bones.findIndex((bone) => bone.name === name)
              )
            )
            bracerGeometry = meshes.map((mesh) => {
              mesh.geometry = mesh.geometry.clone()
              mesh.geometry.computeBoundingBox()
              const fitCenter: unknown = mesh.userData.fit_center
              const center =
                Array.isArray(fitCenter) &&
                fitCenter.length === 3 &&
                fitCenter.every(Number.isFinite)
                  ? new THREE.Vector3().fromArray(fitCenter as number[])
                  : mesh.geometry.boundingBox!.getCenter(new THREE.Vector3())
              const attribute = mesh.geometry.getAttribute(
                'position'
              ) as THREE.BufferAttribute
              const normal = mesh.geometry.getAttribute('normal') as
                | THREE.BufferAttribute
                | undefined
              const tangent = mesh.geometry.getAttribute('tangent') as
                | THREE.BufferAttribute
                | undefined
              return {
                mesh,
                positions: new Float32Array(attribute.array),
                normals: normal && new Float32Array(normal.array),
                tangents: tangent && new Float32Array(tangent.array),
                center,
                side: center.x < 0 ? -1 : 1,
              }
            })
            updateBracerFit()
          }
          if (sleeves) {
            outfitMask.register('glove-sleeves', sleeveMask, meshes[0])
            sleeveMaskRegistered = true
            outfitMask.setEquipped('glove-sleeves', knightGlovesEquipped)
          }
          if (slot === 'boots') {
            let body: THREE.SkinnedMesh | undefined
            model.traverse((node) => {
              const association = base.parser.associations.get(node)
              if (
                node instanceof THREE.SkinnedMesh &&
                association?.meshes === 0 &&
                association.primitives === 0
              )
                body = node
            })
            if (!body) throw new Error('부츠의 기준 몸체를 찾지 못했습니다.')
            entry.ground = femaleBootGround(
              model,
              body,
              meshes,
              source.userData.heel_pose
            )
          }
          for (const mesh of meshes) {
            mesh.visible = false
            if (slot === 'gloves' || slot === 'helmet')
              mesh.frustumCulled = false
            collectMaterials(mesh)
          }
          resources.push(source)
          for (const material of materials)
            material.wireframe = wireframe.checked
        })().catch((error: unknown) => {
          outfitMask.unregister(maskName(outfit))
          if (sleeves) {
            outfitMask.unregister('glove-sleeves')
            sleeveMaskRegistered = false
          }
          for (const mesh of entry.meshes ?? []) mesh.removeFromParent()
          if (source) disposeObjects([source])
          loaded.delete(outfit)
          throw error
        })
      }
      return entry.pending
    }
    if (slot === 'boots')
      loadPlateBoots = async () => {
        await load('knight')
        plateGround = loaded.get('knight')?.ground
      }
    const show = (outfit: string) => {
      if (slot === 'boots') {
        bootGround?.restore()
        bootGround = loaded.get(outfit)?.ground
      }
      if (slot === 'gloves') {
        bracerMaskEnabled = outfit === 'valkyrie' && bracerHideSkin.checked
        clearTimeout(bracerMaskTimer)
        if (bracerMaskEnabled) refreshBracerMask()
        else outfitMask.unregister('gloves-valkyrie')
        if (
          outfit === 'valkyrie' &&
          bracerEditing &&
          hasStoredBracerFit &&
          !bracerSnapshotSent
        ) {
          bracerSnapshotSent = true
          void saveBracerFit(true)
        }
      }
      for (const [name, entry] of loaded) {
        if (!entry.meshes) continue
        if (!(slot === 'gloves' && name === 'valkyrie'))
          outfitMask.setEquipped(
            maskName(name),
            name === outfit && !skinShown.has(maskName(name))
          )
        for (const mesh of entry.meshes) mesh.visible = name === outfit
      }
      underwearColor(
        !!model.getObjectByName('bottom_female_rogue')?.visible,
        !!model.getObjectByName('bottom_female_caveman')?.visible
      )
    }
    const update = async () => {
      const requested = select.value
      if (slot === 'gloves') bracerPanel.hidden = requested !== 'valkyrie'
      try {
        if (requested !== 'none' && !loaded.get(requested)?.meshes) {
          status.textContent = '갑옷을 불러오는 중…'
          await load(requested)
        }
        if (slot === 'helmet') {
          do {
            for (const mesh of await loadHair(hairSelect.value, requested))
              collectMaterials(mesh)
            if (disposed() || select.value !== requested) return
          } while (!hairOptions[hairKey(hairSelect.value, requested)])
          for (const material of materials)
            material.wireframe = wireframe.checked
        }
        if (disposed() || select.value !== requested) return
        const equipped = requested !== 'none'
        if (slot === 'boots') bootsEquipped = equipped
        if (slot === 'helmet') {
          helmetOutfit = requested
          helmetEquipped = equipped
          updateHair()
          if (
            (document.getElementById('female-camera') as HTMLSelectElement)
              .value === 'face'
          )
            reset()
        }
        if (slot === 'gloves') {
          knightGlovesEquipped = requested === 'knight'
          if (sleeveMaskRegistered)
            outfitMask.setEquipped('glove-sleeves', knightGlovesEquipped)
        }
        show(requested)
        updateFitPanel()
        await updateGarters()
        if (disposed() || select.value !== requested) return
        updateModularPoseCorrectives(model, motion.value)
        updateGround()
        updateStats()
        updateStatus()
      } catch {
        if (disposed() || select.value !== requested) return
        select.value = 'none'
        if (slot === 'gloves') bracerPanel.hidden = true
        if (slot === 'boots') bootsEquipped = false
        if (slot === 'helmet') {
          helmetOutfit = 'none'
          helmetEquipped = false
          updateHair()
          if (
            (document.getElementById('female-camera') as HTMLSelectElement)
              .value === 'face'
          )
            reset()
        }
        if (slot === 'gloves') {
          knightGlovesEquipped = false
          if (sleeveMaskRegistered)
            outfitMask.setEquipped('glove-sleeves', false)
        }
        show('none')
        updateFitPanel()
        await updateGarters()
        updateGround()
        updateStats()
        status.textContent = '갑옷을 불러오지 못했습니다. 다시 선택해 주세요.'
      }
    }
    select.onchange = update
    return update
  })
  const updateStats = () => {
    let triangles = 0
    model.traverseVisible((node) => {
      if (node instanceof THREE.Mesh)
        triangles +=
          (node.geometry.index?.count ??
            node.geometry.attributes.position.count) / 3
    })
    document.getElementById('female-stats')!.textContent =
      `${triangles.toLocaleString()}삼각형 · ${bones.length}개 본 · ${animations.length}가지 동작`
  }
  let appliedHair = hairSelect.value
  hairSelect.onchange = async () => {
    const requested = hairSelect.value
    try {
      const meshes = await loadHair(requested)
      if (disposed()) return
      for (const mesh of meshes) collectMaterials(mesh)
      for (const material of materials) material.wireframe = wireframe.checked
      if (hairSelect.value !== requested) return
      appliedHair = requested
      hairFitControls.hidden = requested !== 'warrior'
      updateHair()
      updateStats()
      updateStatus()
    } catch {
      if (disposed() || hairSelect.value !== requested) return
      hairSelect.value = appliedHair
      status.textContent = '헤어를 불러오지 못했습니다. 다시 선택해 주세요.'
    }
  }
  const host = document.getElementById('viewport')!
  const renderer = new THREE.WebGLRenderer({ antialias: true })
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2))
  renderer.setClearColor('#17252d')
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  host.append(renderer.domElement)
  cleanup.push(() => {
    renderer.setAnimationLoop(null)
    renderer.dispose()
    renderer.domElement.remove()
  })
  const room = new RoomEnvironment()
  const pmrem = new THREE.PMREMGenerator(renderer)
  const environment = pmrem.fromScene(room)
  room.dispose()
  pmrem.dispose()
  scene.environment = environment.texture
  scene.environmentIntensity = 0.65
  cleanup.push(() => environment.dispose())
  scene.add(new THREE.HemisphereLight('#e1ecf2', '#62716b', 2))
  const key = new THREE.DirectionalLight('#fff1e0', 2.5)
  key.position.set(-3, 6, 5)
  const fill = new THREE.DirectionalLight('#b1cbe8', 1.6)
  fill.position.set(3, 3, -3)
  scene.add(key, fill)
  const floor = new THREE.Mesh(
    new THREE.CircleGeometry(3.6, 64),
    new THREE.MeshStandardMaterial({ color: '#273940', roughness: 1 })
  )
  floor.rotation.x = -Math.PI / 2
  floor.position.y = -0.0036
  scene.add(floor)
  // Orthographic side view of the feet under the canvas, with a 1 cm ruler behind the boots on layer 1.
  const sideFeet = document.getElementById('side-feet') as HTMLInputElement
  const SIDE_HEIGHT = 0.16
  const sideView = document.createElement('section')
  sideView.className = 'side-feet-view'
  sideView.setAttribute('aria-label', '발 옆모습 확대')
  host.after(sideView)
  const sideRenderer = new THREE.WebGLRenderer({ antialias: true })
  sideRenderer.setPixelRatio(renderer.getPixelRatio())
  sideRenderer.setClearColor('#17252d')
  sideRenderer.toneMapping = renderer.toneMapping
  sideView.append(sideRenderer.domElement)
  cleanup.push(() => {
    sideRenderer.dispose()
    sideView.remove()
  })
  const sideCamera = new THREE.OrthographicCamera(-1, 1, 0.13, -0.03, 0.01, 6)
  sideCamera.rotation.y = Math.PI / 2
  sideCamera.layers.enable(1)
  const ruler = new THREE.Group()
  for (let cm = 0; cm <= 12; cm++) {
    const color = cm === 0 ? '#ff4040' : cm % 5 === 0 ? '#9fb0b6' : '#3d5058'
    const line = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints([
        new THREE.Vector3(-0.5, cm / 100, -1),
        new THREE.Vector3(-0.5, cm / 100, 1),
      ]),
      new THREE.LineBasicMaterial({ color })
    )
    line.layers.set(1)
    ruler.add(line)
  }
  scene.add(ruler)
  cleanup.push(() => disposeObjects([ruler]))
  const footCenter = new THREE.Vector3()
  const footPosition = new THREE.Vector3()
  const sideSize = new THREE.Vector2()
  const renderSideFeet = () => {
    sideView.hidden = !sideFeet.checked || !bootsEquipped
    if (sideView.hidden) return
    const width = sideView.clientWidth
    const height = sideView.clientHeight
    // It only fills space the settings panel leaves under the canvas.
    if (height < 40) return
    if (width !== sideSize.x || height !== sideSize.y) {
      sideSize.set(width, height)
      sideRenderer.setSize(width, height)
    }
    const aspect = width / height
    footCenter.set(0, 0, 0)
    for (const name of ['LeftFoot', 'RightFoot'])
      footCenter.add(
        model.getObjectByName(name)!.getWorldPosition(footPosition)
      )
    footCenter.multiplyScalar(0.5)
    sideCamera.position.set(3, 0, footCenter.z)
    ruler.position.z = footCenter.z
    sideCamera.left = (-SIDE_HEIGHT * aspect) / 2
    sideCamera.right = (SIDE_HEIGHT * aspect) / 2
    sideCamera.updateProjectionMatrix()
    sideRenderer.render(scene, sideCamera)
  }
  const camera = new THREE.PerspectiveCamera(35, 1, 0.03, 36)
  const controls = new OrbitControls(camera, renderer.domElement)
  controls.enableDamping = true
  setupSkinBrush()
  controls.minDistance = 0.144
  controls.maxDistance = 9
  cleanup.push(() => controls.dispose())
  const views: Record<string, { target: number[]; position: number[] }> = {
    face: { target: [0, 1.6488, 0], position: [0, 1.6668, 0.648] },
    upper: { target: [0, 1.368, 0], position: [0, 1.404, 1.44] },
    lower: { target: [0, 0.936, 0], position: [0, 0.99, 1.44] },
    'lower-back': { target: [0, 0.936, 0], position: [0, 0.99, -1.44] },
    full: { target: [0, 0.9, 0], position: [0, 0.99, 3.42] },
    side: { target: [0, 0.9, 0], position: [3.42, 0.99, 0] },
    back: { target: [0, 0.9, 0], position: [0, 0.99, -3.42] },
    feet: { target: [0, 0.117, 0], position: [0.432, 0.288, 0.81] },
    boots: { target: [0, 0.36, 0], position: [0.25, 0.42, 1.65] },
    'boots-back': { target: [0, 0.36, 0], position: [0, 0.42, -1.65] },
    'boots-side': { target: [0, 0.36, 0], position: [1.65, 0.42, 0] },
    'right-hand': {
      target: [-0.216, 0.864, 0.027],
      position: [-0.81, 0.882, 0.234],
    },
    'left-hand': {
      target: [0.261, 0.864, -0.018],
      position: [0.855, 0.882, 0.189],
    },
  }
  const select = document.getElementById('female-camera') as HTMLSelectElement
  const wingsToggle = document.getElementById(
    'female-wings'
  ) as HTMLInputElement
  wingsToggle.checked =
    new URLSearchParams(location.search).get('wings') === '1'
  const requested = new URLSearchParams(location.search).get('camera')
  select.value =
    requested && Object.hasOwn(views, requested) ? requested : 'full'
  const reset = () => {
    const view = views[select.value]
    const aspect = host.clientWidth / host.clientHeight
    controls.target.fromArray(view.target)
    camera.position.fromArray(view.position)
    if (wingsToggle.checked && ['full', 'back'].includes(select.value)) {
      controls.target.y = 1.1
      camera.position.y = 1.1
      camera.position.z *= 1.4
    }
    if (select.value === 'face' && helmetEquipped) {
      controls.target.set(0, 1.7478, 0)
      camera.position.set(0, 1.7658, 0.864)
    }
    if (select.value === 'full' && torch.visible) {
      controls.target.y = 1.08
      camera.position.set(0, 1.17, 4.14)
    }
    camera.position
      .sub(controls.target)
      .multiplyScalar(Math.max(1, 1 / aspect))
      .add(controls.target)
    if (select.value.endsWith('-hand')) {
      model.updateMatrixWorld(true)
      const side = select.value === 'right-hand' ? 'Right' : 'Left'
      const wrist = model
        .getObjectByName(`${side}Hand`)!
        .getWorldPosition(new THREE.Vector3())
      const tip = model
        .getObjectByName(`${side}HandMiddle4`)!
        .getWorldPosition(new THREE.Vector3())
      const target = wrist.add(tip).multiplyScalar(0.5)
      camera.position.add(target.clone().sub(controls.target))
      controls.target.copy(target)
    }
    controls.update()
  }
  select.onchange = reset
  document.getElementById('female-reset')!.onclick = reset
  const wireframe = document.getElementById(
    'female-wireframe'
  ) as HTMLInputElement
  wireframe.onchange = () => {
    for (const material of materials) material.wireframe = wireframe.checked
  }
  const resize = () => {
    camera.aspect = host.clientWidth / host.clientHeight
    camera.updateProjectionMatrix()
    renderer.setSize(host.clientWidth, host.clientHeight)
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  cleanup.push(() => observer.disconnect())
  resize()
  reset()
  const params = new URLSearchParams(location.search)
  const requestedMotion = params.get('motion') ?? 'idle1'
  const requestedEquipment = params.get('equipment') ?? ''
  equipment.value = [
    'unarmed',
    'armed',
    'torch',
    'armed-torch',
    'shield',
    'armed-shield',
  ].includes(requestedEquipment)
    ? requestedEquipment
    : requestedMotion === 'idle_armed'
      ? 'armed'
      : 'unarmed'
  motion.value =
    requestedMotion === 'idle_armed'
      ? 'idle1'
      : requestedMotion === 'rest' ||
          animations.some((clip) => clip.name === requestedMotion)
        ? requestedMotion
        : 'idle1'
  play()
  const requestedTime = Number(params.get('time'))
  if (params.has('time') && Number.isFinite(requestedTime) && action) {
    time.value = String(THREE.MathUtils.clamp(requestedTime, 0, 1))
    time.dispatchEvent(new Event('input'))
  }
  void loadPlateBoots()
  let previousTime = performance.now()
  renderer.setAnimationLoop((now) => {
    const delta = Math.min((now - previousTime) / 1000, 0.05)
    previousTime = now
    if (!paused && action) {
      updateAnimation(delta)
      time.value = String(action.time / action.getClip().duration)
    }
    controls.update()
    updateGround()
    model.updateMatrixWorld(true)
    updateSoleGap(now)
    updateValkyrieWings(model, paused ? 0 : delta, action)
    updatePeltPhysics(model, paused ? 0 : delta)
    renderer.render(scene, camera)
    renderSideFeet()
  })
  const fieldset = document.getElementById(
    'female-controls'
  ) as HTMLFieldSetElement
  let wingMeshes: THREE.Mesh[] | undefined
  cleanup.push(() => disposeValkyrieWings(model))
  let pendingWings: Promise<void> | undefined
  const updateWings = async () => {
    try {
      if (wingsToggle.checked && !wingMeshes) {
        status.textContent = '날개를 불러오는 중…'
        pendingWings ??= loader
          .loadAsync('/__modular-character/parts/female_valkyrie_wings.glb')
          .then(({ scene: source }) => {
            if (disposed()) {
              disposeObjects([source])
              return
            }
            resources.push(source)
            const meshes = bindModularPart(model, source)
            wingMeshes = attachValkyrieWings(model, meshes)
            for (const mesh of wingMeshes) {
              mesh.frustumCulled = false
              collectMaterials(mesh)
            }
            for (const material of materials)
              material.wireframe = wireframe.checked
          })
          .finally(() => {
            pendingWings = undefined
          })
        await pendingWings
      }
      if (disposed()) return
      for (const mesh of wingMeshes ?? []) mesh.visible = wingsToggle.checked
      reset()
      updateStats()
      updateStatus()
    } catch {
      if (disposed()) return
      wingsToggle.checked = false
      status.textContent = '날개를 불러오지 못했습니다.'
    }
  }
  wingsToggle.onchange = updateWings
  fieldset.disabled = false
  await Promise.all([...outfitUpdates.map((update) => update()), updateWings()])
  if (disposed()) return
  document.body.dataset.ready = 'true'
}
