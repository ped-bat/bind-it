<script>
  import Icon from "$lib/components/ui/Icon.svelte";

  /** @type {{ width?: number, height?: number }} */
  let { width = 34, height = 29 } = $props();
</script>

<Icon name="logomark" class="logo-mark" {width} {height} />
