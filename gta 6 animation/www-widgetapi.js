(function(){'use strict';var n,aa=typeof Object.create=="function"?Object.create:function(a){function b(){}
b.prototype=a;return new b},p=typeof Object.defineProperties=="function"?Object.defineProperty:function(a,b,c){if(a==Array.prototype||a==Object.prototype)return a;
a[b]=c.value;return a};
function ba(a){a=["object"==typeof globalThis&&globalThis,a,"object"==typeof window&&window,"object"==typeof self&&self,"object"==typeof global&&global];for(var b=0;b<a.length;++b){var c=a[b];if(c&&c.Math==Math)return c}throw Error("Cannot find global object");}
var q=ba(this);function r(a,b){if(b)a:{var c=q;a=a.split(".");for(var d=0;d<a.length-1;d++){var h=a[d];if(!(h in c))break a;c=c[h]}a=a[a.length-1];d=c[a];b=b(d);b!=d&&b!=null&&p(c,a,{configurable:!0,writable:!0,value:b})}}
var t;if(typeof Object.setPrototypeOf=="function")t=Object.setPrototypeOf;else{var v;a:{var ca={a:!0},da={};try{da.__proto__=ca;v=da.a;break a}catch(a){}v=!1}t=v?function(a,b){a.__proto__=b;if(a.__proto__!==b)throw new TypeError(a+" is not extensible");return a}:null}var ea=t;
function fa(a){var b=0;return function(){return b<a.length?{done:!1,value:a[b++]}:{done:!0}}}
function x(a){var b=typeof Symbol!="undefined"&&Symbol.iterator&&a[Symbol.iterator];if(b)return b.call(a);if(typeof a.length=="number")return{next:fa(a)};throw Error(String(a)+" is not an iterable or ArrayLike");}
r("Symbol",function(a){function b(k){if(this instanceof b)throw new TypeError("Symbol is not a constructor");return new c(d+(k||"")+"_"+h++,k)}
function c(k,f){this.g=k;p(this,"description",{configurable:!0,writable:!0,value:f})}
if(a)return a;c.prototype.toString=function(){return this.g};
var d="jscomp_symbol_"+(Math.random()*1E9>>>0)+"_",h=0;return b});
r("Symbol.iterator",function(a){if(a)return a;a=Symbol("Symbol.iterator");p(Array.prototype,a,{configurable:!0,writable:!0,value:function(){return ha(fa(this))}});
return a});
function ha(a){a={next:a};a[Symbol.iterator]=function(){return this};
return a}
r("Promise",function(a){function b(f){this.h=0;this.i=void 0;this.g=[];this.v=!1;var e=this.j();try{f(e.resolve,e.reject)}catch(g){e.reject(g)}}
function c(){this.g=null}
function d(f){return f instanceof b?f:new b(function(e){e(f)})}
if(a)return a;c.prototype.h=function(f){if(this.g==null){this.g=[];var e=this;this.i(function(){e.l()})}this.g.push(f)};
var h=q.setTimeout;c.prototype.i=function(f){h(f,0)};
c.prototype.l=function(){for(;this.g&&this.g.length;){var f=this.g;this.g=[];for(var e=0;e<f.length;++e){var g=f[e];f[e]=null;try{g()}catch(l){this.j(l)}}}this.g=null};
c.prototype.j=function(f){this.i(function(){throw f;})};
b.prototype.j=function(){function f(l){return function(m){g||(g=!0,l.call(e,m))}}
var e=this,g=!1;return{resolve:f(this.K),reject:f(this.l)}};
b.prototype.K=function(f){if(f===this)this.l(new TypeError("A Promise cannot resolve to itself"));else if(f instanceof b)this.M(f);else{a:switch(typeof f){case "object":var e=f!=null;break a;case "function":e=!0;break a;default:e=!1}e?this.J(f):this.o(f)}};
b.prototype.J=function(f){var e=void 0;try{e=f.then}catch(g){this.l(g);return}typeof e=="function"?this.N(e,f):this.o(f)};
b.prototype.l=function(f){this.A(2,f)};
b.prototype.o=function(f){this.A(1,f)};
b.prototype.A=function(f,e){if(this.h!=0)throw Error("Cannot settle("+f+", "+e+"): Promise already settled in state"+this.h);this.h=f;this.i=e;this.h===2&&this.L();this.C()};
b.prototype.L=function(){var f=this;h(function(){if(f.I()){var e=q.console;typeof e!=="undefined"&&e.error(f.i)}},1)};
b.prototype.I=function(){if(this.v)return!1;var f=q.CustomEvent,e=q.Event,g=q.dispatchEvent;if(typeof g==="undefined")return!0;typeof f==="function"?f=new f("unhandledrejection",{cancelable:!0}):typeof e==="function"?f=new e("unhandledrejection",{cancelable:!0}):(f=q.document.createEvent("CustomEvent"),f.initCustomEvent("unhandledrejection",!1,!0,f));f.promise=this;f.reason=this.i;return g(f)};
b.prototype.C=function(){if(this.g!=null){for(var f=0;f<this.g.length;++f)k.h(this.g[f]);this.g=null}};
var k=new c;b.prototype.M=function(f){var e=this.j();f.B(e.resolve,e.reject)};
b.prototype.N=function(f,e){var g=this.j();try{f.call(e,g.resolve,g.reject)}catch(l){g.reject(l)}};
b.prototype.then=function(f,e){function g(w,z){return typeof w=="function"?function(P){try{l(w(P))}catch(Q){m(Q)}}:z}
var l,m,u=new b(function(w,z){l=w;m=z});
this.B(g(f,l),g(e,m));return u};
b.prototype.catch=function(f){return this.then(void 0,f)};
b.prototype.B=function(f,e){function g(){switch(l.h){case 1:f(l.i);break;case 2:e(l.i);break;default:throw Error("Unexpected state: "+l.h);}}
var l=this;this.g==null?k.h(g):this.g.push(g);this.v=!0};
b.resolve=d;b.reject=function(f){return new b(function(e,g){g(f)})};
b.race=function(f){return new b(function(e,g){for(var l=x(f),m=l.next();!m.done;m=l.next())d(m.value).B(e,g)})};
b.all=function(f){var e=x(f),g=e.next();return g.done?d([]):new b(function(l,m){function u(P){return function(Q){w[P]=Q;z--;z==0&&l(w)}}
var w=[],z=0;do w.push(void 0),z++,d(g.value).B(u(w.length-1),m),g=e.next();while(!g.done)})};
return b});
function y(a,b){return Object.prototype.hasOwnProperty.call(a,b)}
var ia=typeof Object.assign=="function"?Object.assign:function(a,b){if(a==null)throw new TypeError("No nullish arg");a=Object(a);for(var c=1;c<arguments.length;c++){var d=arguments[c];if(d)for(var h in d)y(d,h)&&(a[h]=d[h])}return a};
r("Object.assign",function(a){return a||ia});
r("Symbol.dispose",function(a){return a?a:Symbol("Symbol.dispose")});
r("WeakMap",function(a){function b(g){this.g=(e+=Math.random()+1).toString();if(g){g=x(g);for(var l;!(l=g.next()).done;)l=l.value,this.set(l[0],l[1])}}
function c(){}
function d(g){var l=typeof g;return l==="object"&&g!==null||l==="function"}
function h(g){if(!y(g,f)){var l=new c;p(g,f,{value:l})}}
function k(g){var l=Object[g];l&&(Object[g]=function(m){if(m instanceof c)return m;Object.isExtensible(m)&&h(m);return l(m)})}
if(function(){if(!a||!Object.seal)return!1;try{var g=Object.seal({}),l=Object.seal({}),m=new a([[g,2],[l,3]]);if(m.get(g)!=2||m.get(l)!=3)return!1;m.delete(g);m.set(l,4);return!m.has(g)&&m.get(l)==4}catch(u){return!1}}())return a;
var f="$jscomp_hidden_"+Math.random();k("freeze");k("preventExtensions");k("seal");var e=0;b.prototype.set=function(g,l){if(!d(g))throw Error("Invalid WeakMap key");h(g);if(!y(g,f))throw Error("WeakMap key fail: "+g);g[f][this.g]=l;return this};
b.prototype.get=function(g){return d(g)&&y(g,f)?g[f][this.g]:void 0};
b.prototype.has=function(g){return d(g)&&y(g,f)&&y(g[f],this.g)};
b.prototype.delete=function(g){return d(g)&&y(g,f)&&y(g[f],this.g)?delete g[f][this.g]:!1};
return b});
r("Map",function(a){function b(){var e={};return e.previous=e.next=e.head=e}
function c(e,g){var l=e[1];return ha(function(){if(l){for(;l.head!=e[1];)l=l.previous;for(;l.next!=l.head;)return l=l.next,{done:!1,value:g(l)};l=null}return{done:!0,value:void 0}})}
function d(e,g){var l=g&&typeof g;l=="object"||l=="function"?k.has(g)?l=k.get(g):(l=""+ ++f,k.set(g,l)):l="p_"+g;var m=e[0][l];if(m&&y(e[0],l))for(e=0;e<m.length;e++){var u=m[e];if(g!==g&&u.key!==u.key||g===u.key)return{id:l,list:m,index:e,entry:u}}return{id:l,list:m,index:-1,entry:void 0}}
function h(e){this[0]={};this[1]=b();this.size=0;if(e){e=x(e);for(var g;!(g=e.next()).done;)g=g.value,this.set(g[0],g[1])}}
if(function(){if(!a||typeof a!="function"||!a.prototype.entries||typeof Object.seal!="function")return!1;try{var e=Object.seal({x:4}),g=new a(x([[e,"s"]]));if(g.get(e)!="s"||g.size!=1||g.get({x:4})||g.set({x:4},"t")!=g||g.size!=2)return!1;var l=g.entries(),m=l.next();if(m.done||m.value[0]!=e||m.value[1]!="s")return!1;m=l.next();return m.done||m.value[0].x!=4||m.value[1]!="t"||!l.next().done?!1:!0}catch(u){return!1}}())return a;
var k=new WeakMap;h.prototype.set=function(e,g){e=e===0?0:e;var l=d(this,e);l.list||(l.list=this[0][l.id]=[]);l.entry?l.entry.value=g:(l.entry={next:this[1],previous:this[1].previous,head:this[1],key:e,value:g},l.list.push(l.entry),this[1].previous.next=l.entry,this[1].previous=l.entry,this.size++);return this};
h.prototype.delete=function(e){e=d(this,e);return e.entry&&e.list?(e.list.splice(e.index,1),e.list.length||delete this[0][e.id],e.entry.previous.next=e.entry.next,e.entry.next.previous=e.entry.previous,e.entry.head=null,this.size--,!0):!1};
h.prototype.clear=function(){this[0]={};this[1]=this[1].previous=b();this.size=0};
h.prototype.has=function(e){return!!d(this,e).entry};
h.prototype.get=function(e){return(e=d(this,e).entry)&&e.value};
h.prototype.entries=function(){return c(this,function(e){return[e.key,e.value]})};
h.prototype.keys=function(){return c(this,function(e){return e.key})};
h.prototype.values=function(){return c(this,function(e){return e.value})};
h.prototype.forEach=function(e,g){for(var l=this.entries(),m;!(m=l.next()).done;)m=m.value,e.call(g,m[1],m[0],this)};
h.prototype[Symbol.iterator]=h.prototype.entries;var f=0;return h});
r("Set",function(a){function b(c){this.g=new Map;if(c){c=x(c);for(var d;!(d=c.next()).done;)this.add(d.value)}this.size=this.g.size}
if(function(){if(!a||typeof a!="function"||!a.prototype.entries||typeof Object.seal!="function")return!1;try{var c=Object.seal({x:4}),d=new a(x([c]));if(!d.has(c)||d.size!=1||d.add(c)!=d||d.size!=1||d.add({x:4})!=d||d.size!=2)return!1;var h=d.entries(),k=h.next();if(k.done||k.value[0]!=c||k.value[1]!=c)return!1;k=h.next();return k.done||k.value[0]==c||k.value[0].x!=4||k.value[1]!=k.value[0]?!1:h.next().done}catch(f){return!1}}())return a;
b.prototype.add=function(c){c=c===0?0:c;this.g.set(c,c);this.size=this.g.size;return this};
b.prototype.delete=function(c){c=this.g.delete(c);this.size=this.g.size;return c};
b.prototype.clear=function(){this.g.clear();this.size=0};
b.prototype.has=function(c){return this.g.has(c)};
b.prototype.entries=function(){return this.g.entries()};
b.prototype.values=function(){return this.g.values()};
b.prototype.keys=b.prototype.values;b.prototype[Symbol.iterator]=b.prototype.values;b.prototype.forEach=function(c,d){var h=this;this.g.forEach(function(k){return c.call(d,k,k,h)})};
return b});
r("Array.prototype.find",function(a){return a?a:function(b,c){a:{var d=this;d instanceof String&&(d=String(d));for(var h=d.length,k=0;k<h;k++){var f=d[k];if(b.call(c,f,k,d)){b=f;break a}}b=void 0}return b}});
r("Array.from",function(a){return a?a:function(b,c,d){c=c!=null?c:function(e){return e};
var h=[],k=typeof Symbol!="undefined"&&Symbol.iterator&&b[Symbol.iterator];if(typeof k=="function"){b=k.call(b);for(var f=0;!(k=b.next()).done;)h.push(c.call(d,k.value,f++))}else for(k=b.length,f=0;f<k;f++)h.push(c.call(d,b[f],f));return h}});/*

 Copyright The Closure Library Authors.
 SPDX-License-Identifier: Apache-2.0
*/
var A=this||self;function B(a){var b=typeof a;return b=="object"&&a!=null||b=="function"}
function ja(a){return Object.prototype.hasOwnProperty.call(a,C)&&a[C]||(a[C]=++ka)}
var C="closure_uid_"+(Math.random()*1E9>>>0),ka=0;function D(a,b){a=a.split(".");for(var c=A,d;a.length&&(d=a.shift());)a.length||b===void 0?c[d]&&c[d]!==Object.prototype[d]?c=c[d]:c=c[d]={}:c[d]=b}
function la(a,b){function c(){}
c.prototype=b.prototype;a.H=b.prototype;a.prototype=new c;a.prototype.constructor=a;a.W=function(d,h,k){for(var f=Array(arguments.length-2),e=2;e<arguments.length;e++)f[e-2]=arguments[e];return b.prototype[h].apply(d,f)}}
;var ma=Array.prototype.indexOf?function(a,b){return Array.prototype.indexOf.call(a,b,void 0)}:function(a,b){if(typeof a==="string")return typeof b!=="string"||b.length!=1?-1:a.indexOf(b,0);
for(var c=0;c<a.length;c++)if(c in a&&a[c]===b)return c;return-1},E=Array.prototype.forEach?function(a,b,c){Array.prototype.forEach.call(a,b,c)}:function(a,b,c){for(var d=a.length,h=typeof a==="string"?a.split(""):a,k=0;k<d;k++)k in h&&b.call(c,h[k],k,a)};
function na(a,b){b=ma(a,b);b>=0&&Array.prototype.splice.call(a,b,1)}
function oa(a){return Array.prototype.concat.apply([],arguments)}
function pa(a){var b=a.length;if(b>0){for(var c=Array(b),d=0;d<b;d++)c[d]=a[d];return c}return[]}
;function qa(a,b){this.i=a;this.j=b;this.h=0;this.g=null}
qa.prototype.get=function(){if(this.h>0){this.h--;var a=this.g;this.g=a.next;a.next=null}else a=this.i();return a};function ra(a){A.setTimeout(function(){throw a;},0)}
;function F(){this.h=this.g=null}
F.prototype.add=function(a,b){var c=sa.get();c.set(a,b);this.h?this.h.next=c:this.g=c;this.h=c};
F.prototype.remove=function(){var a=null;this.g&&(a=this.g,this.g=this.g.next,this.g||(this.h=null),a.next=null);return a};
var sa=new qa(function(){return new G},function(a){return a.reset()});
function G(){this.next=this.scope=this.g=null}
G.prototype.set=function(a,b){this.g=a;this.scope=b;this.next=null};
G.prototype.reset=function(){this.next=this.scope=this.g=null};var H,I=!1,ta=new F;function ua(a){H||va();I||(H(),I=!0);ta.add(a,void 0)}
function va(){var a=Promise.resolve(void 0);H=function(){a.then(wa)}}
function wa(){for(var a;a=ta.remove();){try{a.g.call(a.scope)}catch(c){ra(c)}var b=sa;b.j(a);b.h<100&&(b.h++,a.next=b.g,b.g=a)}I=!1}
;function J(){this.i=this.i;this.j=this.j}
J.prototype.i=!1;J.prototype.dispose=function(){this.i||(this.i=!0,this.D())};
J.prototype[Symbol.dispose]=function(){this.dispose()};
J.prototype.addOnDisposeCallback=function(a,b){this.i?b!==void 0?a.call(b):a():(this.j||(this.j=[]),b&&(a=a.bind(b)),this.j.push(a))};
J.prototype.D=function(){if(this.j)for(;this.j.length;)this.j.shift()()};function xa(a){var b={},c;for(c in a)b[c]=a[c];return b}
;var ya=/&/g,za=/</g,Aa=/>/g,Ba=/"/g,Ca=/'/g,Da=/\x00/g,Ea=/[\x00&<>"']/;/*

 Copyright Google LLC
 SPDX-License-Identifier: Apache-2.0
*/
function K(a){this.g=a}
K.prototype.toString=function(){return this.g};
var Fa=new K("about:invalid#zClosurez");function L(a){this.P=a}
function M(a){return new L(function(b){return b.substr(0,a.length+1).toLowerCase()===a+":"})}
var Ga=[M("data"),M("http"),M("https"),M("mailto"),M("ftp"),new L(function(a){return/^[^:]*([/?#]|$)/.test(a)})],Ha=/^\s*(?!javascript:)(?:[\w+.-]+:|[^:/?#]*(?:[/?#]|$))/i;var Ia={V:0,T:1,U:2,0:"FORMATTED_HTML_CONTENT",1:"EMBEDDED_INTERNAL_CONTENT",2:"EMBEDDED_TRUSTED_EXTERNAL_CONTENT"};function N(a,b){b=Error.call(this,a+" cannot be used with intent "+Ia[b]);this.message=b.message;"stack"in b&&(this.stack=b.stack);this.type=a;this.name="TypeCannotBeUsedWithIframeIntentError"}
var O=Error;N.prototype=aa(O.prototype);N.prototype.constructor=N;if(ea)ea(N,O);else for(var R in O)if(R!="prototype")if(Object.defineProperties){var Ja=Object.getOwnPropertyDescriptor(O,R);Ja&&Object.defineProperty(N,R,Ja)}else N[R]=O[R];N.H=O.prototype;function Ka(a){Ea.test(a)&&(a.indexOf("&")!=-1&&(a=a.replace(ya,"&amp;")),a.indexOf("<")!=-1&&(a=a.replace(za,"&lt;")),a.indexOf(">")!=-1&&(a=a.replace(Aa,"&gt;")),a.indexOf('"')!=-1&&(a=a.replace(Ba,"&quot;")),a.indexOf("'")!=-1&&(a=a.replace(Ca,"&#39;")),a.indexOf("\x00")!=-1&&(a=a.replace(Da,"&#0;")));return a}
;function S(a){J.call(this);this.v=1;this.l=[];this.o=0;this.g=[];this.h={};this.A=!!a}
la(S,J);n=S.prototype;n.subscribe=function(a,b,c){var d=this.h[a];d||(d=this.h[a]=[]);var h=this.v;this.g[h]=a;this.g[h+1]=b;this.g[h+2]=c;this.v=h+3;d.push(h);return h};
n.unsubscribe=function(a,b,c){if(a=this.h[a]){var d=this.g;if(a=a.find(function(h){return d[h+1]==b&&d[h+2]==c}))return this.F(a)}return!1};
n.F=function(a){var b=this.g[a];if(b){var c=this.h[b];this.o!=0?(this.l.push(a),this.g[a+1]=function(){}):(c&&na(c,a),delete this.g[a],delete this.g[a+1],delete this.g[a+2])}return!!b};
n.G=function(a,b){var c=this.h[a];if(c){var d=Array(arguments.length-1),h=arguments.length,k;for(k=1;k<h;k++)d[k-1]=arguments[k];if(this.A)for(k=0;k<c.length;k++)h=c[k],La(this.g[h+1],this.g[h+2],d);else{this.o++;try{for(k=0,h=c.length;k<h&&!this.i;k++){var f=c[k];this.g[f+1].apply(this.g[f+2],d)}}finally{if(this.o--,this.l.length>0&&this.o==0)for(;c=this.l.pop();)this.F(c)}}return k!=0}return!1};
function La(a,b,c){ua(function(){a.apply(b,c)})}
n.clear=function(a){if(a){var b=this.h[a];b&&(b.forEach(this.F,this),delete this.h[a])}else this.g.length=0,this.h={}};
n.D=function(){S.H.D.call(this);this.clear();this.l.length=0};var Ma=RegExp("^(?:([^:/?#.]+):)?(?://(?:([^\\\\/?#]*)@)?([^\\\\/?#]*?)(?::([0-9]+))?(?=[\\\\/?#]|$))?([^?#]+)?(?:\\?([^#]*))?(?:#([\\s\\S]*))?$");function Na(a){var b=a.match(Ma);a=b[1];var c=b[2],d=b[3];b=b[4];var h="";a&&(h+=a+":");d&&(h+="//",c&&(h+=c+"@"),h+=d,b&&(h+=":"+b));return h}
function Oa(a,b,c){if(Array.isArray(b))for(var d=0;d<b.length;d++)Oa(a,String(b[d]),c);else b!=null&&c.push(a+(b===""?"":"="+encodeURIComponent(String(b))))}
var Pa=/#|$/;var T={},Qa=[],U=new S,V={};function Ra(){for(var a=x(Qa),b=a.next();!b.done;b=a.next())b=b.value,b()}
function W(a,b){return a.tagName.toLowerCase().substring(0,3)==="yt:"?a.getAttribute(b):a.dataset?a.dataset[b]:a.getAttribute("data-"+b)}
function Sa(a){U.G.apply(U,arguments)}
;function Ta(a){return(a.search("cue")===0||a.search("load")===0)&&a!=="loadModule"}
function Ua(a){return a.search("get")===0||a.search("is")===0}
;var Va=window;
function X(a,b){this.u={};this.playerInfo={};this.videoTitle="";this.j=this.g=null;this.h=0;this.o=!1;this.l=[];this.i=null;this.C={};this.options=null;this.A=this.R.bind(this);if(!a)throw Error("YouTube player element ID required.");this.id=ja(this);b=Object.assign({title:"video player",videoId:"",width:640,height:360},b||{});var c=document;if(a=typeof a==="string"?c.getElementById(a):a){c=a.tagName.toLowerCase()==="iframe";b.host||(b.host=c?Na(a.src):"https://www.youtube.com");this.options=b||{};
b=[this.options,window.YTConfig||{}];for(var d=0;d<b.length;d++)b[d].host&&(b[d].host=b[d].host.toString().replace("http://","https://"));if(!c){b=document.createElement("iframe");c=a.attributes;d=0;for(var h=c.length;d<h;d++){var k=c[d].value;k!=null&&k!==""&&k!=="null"&&b.setAttribute(c[d].name,k)}b.setAttribute("frameBorder","0");b.setAttribute("allowfullscreen","");b.setAttribute("allow","accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share");b.setAttribute("referrerPolicy",
"strict-origin-when-cross-origin");b.setAttribute("title","YouTube "+Y(this,"title"));(c=Y(this,"width"))&&b.setAttribute("width",c.toString());(c=Y(this,"height"))&&b.setAttribute("height",c.toString());this.j=a;(c=a.parentNode)&&c.replaceChild(b,a);a=Wa(this,b);c=""+Y(this,"host")+Xa(this)+"?";d=[];for(var f in a)Oa(f,a[f],d);f=c+d.join("&");if(Va.yt_embedsEnableIframeSrcWithIntent){var e=e===void 0?Ga:e;a:if(e=e===void 0?Ga:e,f instanceof K)e=f;else{for(a=0;a<e.length;++a)if(c=e[a],c instanceof
L&&c.P(f)){e=new K(f);break a}e=void 0}e=e||Fa;b.removeAttribute("srcdoc");f="allow-same-origin allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-storage-access-by-user-activation".split(" ");b.setAttribute("sandbox","");for(a=0;a<f.length;a++)b.sandbox.supports&&!b.sandbox.supports(f[a])||b.sandbox.add(f[a]);if(e instanceof K)if(e instanceof K)e=e.g;else throw Error("");else e=Ha.test(e)?e:void 0;e!==void 0&&(b.src=e);b.sandbox.add("allow-presentation","allow-top-navigation")}else b.src=
f;a=b}this.g=a;this.g.id||(this.g.id="widget"+ja(this.g));T[this.g.id]=this;if(window.postMessage){this.i=new S;Ya(this);b=Y(this,"events");for(var g in b)b.hasOwnProperty(g)&&this.addEventListener(g,b[g]);for(var l in V)V.hasOwnProperty(l)&&Za(this,l)}}}
n=X.prototype;n.setSize=function(a,b){this.g.width=a.toString();this.g.height=b.toString();return this};
n.getIframe=function(){return this.g};
n.addEventListener=function(a,b){var c=b;typeof b==="string"&&(c=function(){window[b].apply(window,arguments)});
if(!c)return this;this.i.subscribe(a,c);$a(this,a);return this};
function Za(a,b){b=b.split(".");if(b.length===2){var c=b[1];"player"===b[0]&&$a(a,c)}}
n.destroy=function(){this.g&&this.g.id&&(T[this.g.id]=null);var a=this.i;a&&typeof a.dispose=="function"&&a.dispose();if(this.j){a=this.j;var b=this.g,c=b.parentNode;c&&c.replaceChild(a,b)}else(a=this.g)&&a.parentNode&&a.parentNode.removeChild(a);Z&&(Z[this.id]=null);this.options=null;this.g&&this.v&&this.g.removeEventListener("load",this.v);this.j=this.g=null};
function ab(a,b,c){c=c||[];c=Array.prototype.slice.call(c);b={event:"command",func:b,args:c};a.o?a.sendMessage(b):a.l.push(b)}
n.R=function(){bb(this)||clearInterval(this.h)};
function bb(a){if(!a.g||!a.g.contentWindow)return!1;a.sendMessage({event:"listening"});return!0}
function Ya(a){cb(a,a.id,String(Y(a,"host")));var b=Number(Va.yt_embedsWidgetPollIntervalMs)||250;a.h=setInterval(a.A,b);a.g&&(a.v=function(){clearInterval(a.h);a.h=setInterval(a.A,b)},a.g.addEventListener("load",a.v))}
function db(a){var b=a.getBoundingClientRect();a=Math.max(0,Math.min(b.bottom,window.innerHeight||document.documentElement.clientHeight)-Math.max(b.top,0))*Math.max(0,Math.min(b.right,window.innerWidth||document.documentElement.clientWidth)-Math.max(b.left,0));a=(b=b.height*b.width)?a/b:0;return document.visibilityState==="hidden"||a<.5?1:a<.75?2:a<.85?3:a<.95?4:a<1?5:6}
function $a(a,b){a.C[b]||(a.C[b]=!0,ab(a,"addEventListener",[b]))}
n.sendMessage=function(a){a.id=this.id;a.channel="widget";a=JSON.stringify(a);var b=Na(this.g.src||"").replace("http:","https:");if(this.g.contentWindow)try{this.g.contentWindow.postMessage(a,b)}catch(c){if(c.name&&c.name==="SyntaxError")c.message&&c.message.indexOf("target origin ''")>0||console&&console.warn&&console.warn(c);else throw c;}else console&&console.warn&&console.warn("The YouTube player is not attached to the DOM. API calls should be made after the onReady event. See more: https://developers.google.com/youtube/iframe_api_reference#Events")};
function Xa(a){if((a=String(Y(a,"videoId")))&&(a.length!==11||!a.match(/^[a-zA-Z0-9\-_]+$/)))throw Error("Invalid video id");return"/embed/"+a}
function Wa(a,b){var c=Y(a,"playerVars");c?c=xa(c):c={};window!==window.top&&document.referrer&&(c.widget_referrer=document.referrer.substring(0,256));var d=Y(a,"embedConfig");if(d){if(B(d))try{d=JSON.stringify(d)}catch(h){console.error("Invalid embed config JSON",h)}c.embed_config=d}c.enablejsapi=window.postMessage?1:0;window.location.host&&(c.origin=window.location.protocol+"//"+window.location.host);c.widgetid=a.id;window.location.href&&E(["debugjs","debugcss"],function(h){var k=window.location.href;
var f=k.search(Pa);b:{var e=0;for(var g=h.length;(e=k.indexOf(h,e))>=0&&e<f;){var l=k.charCodeAt(e-1);if(l==38||l==63)if(l=k.charCodeAt(e+g),!l||l==61||l==38||l==35)break b;e+=g+1}e=-1}if(e<0)k=null;else{g=k.indexOf("&",e);if(g<0||g>f)g=f;e+=h.length+1;k=decodeURIComponent(k.slice(e,g!==-1?g:0).replace(/\+/g," "))}k!==null&&(c[h]=k)});
window.location.href&&(c.forigin=window.location.href);a=window.location.ancestorOrigins;c.aoriginsup=a===void 0?0:1;a&&a.length>0&&(c.aorigins=Array.from(a).join(","));window.document.referrer&&(c.gporigin=window.document.referrer);b&&(c.vf=db(b));return c}
function eb(a,b){if(B(b)){for(var c in b)b.hasOwnProperty(c)&&(a.playerInfo[c]=b[c]);a.playerInfo.hasOwnProperty("videoData")&&(b=a.playerInfo.videoData,b.hasOwnProperty("title")&&b.title?(b=b.title,b!==a.videoTitle&&(a.videoTitle=b,a.g.setAttribute("title",b))):(a.videoTitle="",a.g.setAttribute("title","YouTube "+Y(a,"title"))))}}
function fb(a,b){b=x(b);for(var c=b.next(),d={};!c.done;d={m:void 0},c=b.next())d.m=c.value,a[d.m]||(d.m==="getCurrentTime"?a[d.m]=function(){var h=this.playerInfo.currentTime;if(this.playerInfo.playerState===1){var k=(Date.now()/1E3-this.playerInfo.currentTimeLastUpdated_)*this.playerInfo.playbackRate;k>0&&(h+=Math.min(k,1))}return h}:Ta(d.m)?a[d.m]=function(h){return function(){this.playerInfo={};
this.u={};ab(this,h.m,arguments);return this}}(d):Ua(d.m)?a[d.m]=function(h){return function(){var k=h.m,f=0;
k.search("get")===0?f=3:k.search("is")===0&&(f=2);return this.playerInfo[k.charAt(f).toLowerCase()+k.substring(f+1)]}}(d):a[d.m]=function(h){return function(){ab(this,h.m,arguments);
return this}}(d))}
n.getVideoEmbedCode=function(){var a=""+Y(this,"host")+Xa(this),b=Number(Y(this,"width")),c=Number(Y(this,"height"));if(isNaN(b)||isNaN(c))throw Error("Invalid width or height property");b=Math.floor(b);c=Math.floor(c);var d=this.videoTitle;a=Ka(a);d=Ka(d!=null?d:"YouTube video player");return'<iframe width="'+b+'" height="'+c+'" src="'+a+'" title="'+(d+'" frameborder="0" allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share" referrerpolicy="strict-origin-when-cross-origin" allowfullscreen></iframe>')};
n.getOptions=function(a){return this.u.namespaces?a?this.u[a]?this.u[a].options||[]:[]:this.u.namespaces||[]:[]};
n.getOption=function(a,b){if(this.u.namespaces&&a&&b&&this.u[a])return this.u[a][b]};
function Y(a,b){a=[a.options,window.YTConfig||{}];for(var c=0;c<a.length;c++){var d=a[c][b];if(d!==void 0)return d}return null}
var Z=null,gb=null;function hb(a){if(a.tagName.toLowerCase()!=="iframe"){var b=W(a,"videoid");b&&(b={videoId:b,width:W(a,"width"),height:W(a,"height")},new X(a,b))}}
function cb(a,b,c){Z||(Z={},gb=new Set,ib.addEventListener("message",function(d){a:if(gb.has(d.origin)){try{var h=JSON.parse(d.data)}catch(e){break a}var k=Z[h.id];if(k&&d.origin===k.O)switch(d=k.S,d.o=!0,d.o&&(E(d.l,d.sendMessage,d),d.l.length=0),k=h.event,h=h.info,k){case "apiInfoDelivery":if(B(h))for(var f in h)h.hasOwnProperty(f)&&(d.u[f]=h[f]);break;case "infoDelivery":eb(d,h);break;case "initialDelivery":B(h)&&(clearInterval(d.h),d.playerInfo={},d.u={},fb(d,h.apiInterface),eb(d,h));break;case "alreadyInitialized":clearInterval(d.h);
break;case "readyToListen":bb(d);break;default:d.i.i||(f={target:d,data:h},d.i.G(k,f),Sa("player."+k,f))}}}));
Z[b]={S:a,O:c};gb.add(c)}
var ib=window;D("YT.PlayerState.UNSTARTED",-1);D("YT.PlayerState.ENDED",0);D("YT.PlayerState.PLAYING",1);D("YT.PlayerState.PAUSED",2);D("YT.PlayerState.BUFFERING",3);D("YT.PlayerState.CUED",5);D("YT.get",function(a){return T[a]});
D("YT.scan",Ra);D("YT.subscribe",function(a,b,c){U.subscribe(a,b,c);V[a]=!0;for(var d in T)T.hasOwnProperty(d)&&Za(T[d],a)});
D("YT.unsubscribe",function(a,b,c){U.unsubscribe(a,b,c)});
D("YT.Player",X);X.prototype.destroy=X.prototype.destroy;X.prototype.setSize=X.prototype.setSize;X.prototype.getIframe=X.prototype.getIframe;X.prototype.addEventListener=X.prototype.addEventListener;X.prototype.getVideoEmbedCode=X.prototype.getVideoEmbedCode;X.prototype.getOptions=X.prototype.getOptions;X.prototype.getOption=X.prototype.getOption;Qa.push(function(a){var b=a;b||(b=document);a=pa(b.getElementsByTagName("yt:player"));b=pa((b||document).querySelectorAll(".yt-player"));E(oa(a,b),hb)});
typeof YTConfig!=="undefined"&&YTConfig.parsetags&&YTConfig.parsetags!=="onload"||Ra();var jb=A.onYTReady;jb&&jb();var kb=A.onYouTubeIframeAPIReady;kb&&kb();var lb=A.onYouTubePlayerAPIReady;lb&&lb();}).call(this);
