// Run from web/: derive character textures from the owner's logo.
import sharp from 'sharp';
import { fileURLToPath } from 'node:url';
import { writeFile } from 'node:fs/promises';
const source = fileURLToPath(new URL('../../demo-video/assets/logos/final.jpg', import.meta.url));
const width=300, height=134;
const data=await sharp(source).resize(width,height,{fit:'fill'}).removeAlpha().raw().toBuffer();
// Silhouettes exclude the fresco background and the coin's shadow.
const hands=[
 [[150,1000],[264,829],[306,734],[334,683],[363,649],[437,609],[504,576],[562,557],[604,548],[646,551],[679,563],[686,575],[674,582],[646,578],[630,584],[654,600],[676,610],[684,624],[677,635],[662,636],[640,625],[622,620],[609,629],[622,643],[629,658],[625,677],[611,687],[597,681],[578,688],[550,691],[512,702],[472,710],[439,700],[423,744],[407,827],[383,923],[371,1000]],
 [[1155,0],[1460,0],[1428,58],[1384,118],[1326,170],[1250,211],[1164,248],[1111,280],[1096,317],[1078,351],[1069,391],[1046,414],[1050,437],[1065,460],[1059,469],[1047,464],[1031,444],[1024,426],[1013,436],[1022,462],[1013,486],[1004,501],[989,489],[979,467],[968,455],[955,451],[940,444],[944,427],[956,406],[960,393],[947,402],[933,398],[921,388],[905,397],[885,415],[861,433],[837,449],[824,450],[825,439],[842,420],[858,399],[878,383],[899,372],[920,375],[930,365],[919,358],[920,346],[939,323],[970,300],[992,271],[1026,242],[1064,210],[1097,173],[1124,139],[1150,105],[1164,72],[1163,39]]
];
function inside(x,y,p) {
 let hit=false;
 for(let i=0,j=p.length-1;i<p.length;j=i++) {
  const [xi,yi]=p[i], [xj,yj]=p[j];
  if((yi>y)!==(yj>y)&&x<(xj-xi)*(y-yi)/(yj-yi)+xi) hit=!hit;
 }
 return hit;
}
const chars='.,:;irsXA253hMHGS#9B&@';
function glyph(data,i) {
 const lum=(data[i]*.2126+data[i+1]*.7152+data[i+2]*.0722)/255;
 return chars[Math.round(Math.max(0,Math.min(1,(lum-.13)/.7))*(chars.length-1))];
}
const rows=[];
for(let y=0;y<height;y++) {
 let row='';
 for(let x=0;x<width;x++) row+=hands.some(p=>inside((x+.5)/width*1500,(y+.5)/height*1000,p))?glyph(data,(y*width+x)*3):' ';
 rows.push(row);
}
const size=48;
const coin=await sharp(source).extract({left:676,top:426,width:150,height:150}).resize(size,size).removeAlpha().raw().toBuffer();
const coinRows=[];
for(let y=0;y<size;y++) {
 let row='';
 for(let x=0;x<size;x++) row+=Math.hypot((x+.5-24)/24,(y+.5-24)/24)>1?' ':glyph(coin,(y*size+x)*3);
 coinRows.push(row);
}
await writeFile(new URL('../components/landing/ascii-logo.json',import.meta.url),JSON.stringify({width,height,hands:rows,coin:coinRows}));
