<script lang="ts">
  import { onMount } from 'svelte'
  import { t } from '../i18n'
  import { assetUrl } from '../utils/assetUrl'

  const SLIDES = [
    { key: 'agents', image: 'village' },
    { key: 'housing', image: 'inn' },
    { key: 'world', image: 'world' },
    { key: 'dungeons', image: 'dungeon' },
  ] as const
  const INTERVAL_MS = 6000

  let current = $state(0)
  let started = $state(false)
  let reached = $state(0)
  let paused = $state(false)
  let loaded = $state<boolean[]>(SLIDES.map(() => false))

  const srcset = (image: string) =>
    `${assetUrl(`/landing/${image}-640.webp`)} 640w, ${assetUrl(`/landing/${image}-960.webp`)} 960w`

  function show(i: number) {
    current = (i + SLIDES.length) % SLIDES.length
    reached = Math.max(reached, current)
  }

  onMount(() => {
    const idle =
      window.requestIdleCallback ?? ((cb: () => void) => setTimeout(cb, 200))
    idle(() => (started = true), { timeout: 1500 })
    const timer = setInterval(() => {
      if (
        !paused &&
        !document.hidden &&
        loaded[(current + 1) % SLIDES.length]
      ) {
        show(current + 1)
      }
    }, INTERVAL_MS)
    return () => clearInterval(timer)
  })
</script>

<section
  class="showcase"
  aria-roledescription="carousel"
  aria-label={$t('login.tagline')}
  onpointerenter={() => (paused = true)}
  onpointerleave={() => (paused = false)}
>
  <div class="frame">
    {#each SLIDES as slide, i (slide.key)}
      <figure
        class="slide"
        class:active={i === current}
        aria-hidden={i !== current}
      >
        {#if started && i <= reached + 1}
          <img
            srcset={srcset(slide.image)}
            sizes="(max-width: 600px) 100vw, 640px"
            alt={$t(`login.feature.${slide.key}.title`)}
            class:loaded={loaded[i]}
            onload={() => (loaded[i] = true)}
          />
        {/if}
        <figcaption>
          <strong>{$t(`login.feature.${slide.key}.title`)}</strong>
          <span>{$t(`login.feature.${slide.key}.desc`)}</span>
        </figcaption>
      </figure>
    {/each}
    <div class="dots">
      {#each SLIDES as slide, i (slide.key)}
        <button
          class:active={i === current}
          aria-label={$t(`login.feature.${slide.key}.title`)}
          onclick={() => show(i)}
        ></button>
      {/each}
    </div>
  </div>
</section>

<style>
  .showcase {
    width: min(640px, 100%);
    margin-top: 22px;
    animation: fade-up 0.7s ease-out 0.8s both;
  }

  .frame {
    position: relative;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    background: rgba(12, 10, 30, 0.7);
    border: 1px solid rgba(255, 210, 140, 0.3);
    border-radius: 12px;
    box-shadow:
      0 12px 40px rgba(0, 0, 0, 0.55),
      0 0 24px rgba(246, 199, 122, 0.12);
  }

  .slide {
    position: absolute;
    inset: 0;
    margin: 0;
    opacity: 0;
    transition: opacity 1.2s ease;
  }

  .slide.active {
    opacity: 1;
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    opacity: 0;
    transition:
      opacity 0.6s ease,
      transform 9s ease-out;
  }

  img.loaded {
    opacity: 1;
  }

  .slide.active img {
    transform: scale(1.1) translate(-1.5%, -1%);
  }

  figcaption {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 36px 16px 14px;
    background: linear-gradient(transparent, rgba(8, 6, 20, 0.88));
    text-align: left;
  }

  figcaption strong {
    display: block;
    color: #fbe7c6;
    font-size: 16px;
    font-weight: 600;
    text-shadow: 0 2px 6px rgba(0, 0, 0, 0.8);
  }

  figcaption span {
    display: block;
    margin-top: 3px;
    color: #d7d0e6;
    font-size: 13px;
    line-height: 1.4;
  }

  .slide.active figcaption {
    animation: fade-up 0.8s ease-out 0.3s both;
  }

  .dots {
    position: absolute;
    right: 14px;
    bottom: 16px;
    display: flex;
    gap: 6px;
  }

  .dots button {
    width: 8px;
    height: 8px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    transition:
      width 0.3s,
      background 0.3s;
  }

  .dots button.active {
    width: 20px;
    background: #f6c77a;
  }

  @media (max-width: 600px), (max-height: 700px) {
    .showcase {
      margin-top: 14px;
    }

    figcaption {
      padding: 28px 12px 10px;
    }

    figcaption strong {
      font-size: 14px;
    }

    figcaption span {
      font-size: 11.5px;
      padding-right: 60px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .showcase,
    .slide.active figcaption {
      animation: none;
    }

    .slide.active img {
      transform: none;
    }
  }
</style>
