const fs = require('fs');
let code = fs.readFileSync('E:/007Launcher/src/components/LuaShop.tsx', 'utf8');

code = code.replace(
  "{b.version ? `${b.version} (` : ''}",
  "{b.version ? `${b.version} | ` : ''}"
);

code = code.replace(
  "{b.version ? `)` : ''}",
  "{b.build_date ? ` | ${formatBuildDate(b.build_date)}` : ''}"
);

fs.writeFileSync('E:/007Launcher/src/components/LuaShop.tsx', code);
