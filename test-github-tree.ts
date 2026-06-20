async function getTree() {
  const res = await fetch('https://api.github.com/repos/dangjimmy33-dotcom/0xoLemon-Launcher/git/trees/main?recursive=1')
  const data = await res.json()
  console.log(data.tree.filter((t: any) => t.path.startsWith('src/assets/')).slice(0, 5))
}
getTree()
