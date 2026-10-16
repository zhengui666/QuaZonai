# A strict, dependency-free JSON reader for deployment state. POSIX awk; byte locale.
# Files are parsed completely, duplicate keys and malformed UTF-8 are rejected.
# Modes: get (decoded scalar), raw, canonical, merge (two objects), quote (stdin text).
function die(message) { print "Invalid deployment JSON: " message > "/dev/stderr"; exit 1 }
function ws() { while (substr(doc, pos, 1) ~ /^[ \t\r\n]$/) pos++ }
function hex4(    s,n,i,c) {
    s=substr(doc,pos,4); if(length(s)!=4 || s ~ /[^0-9A-Fa-f]/) die("invalid Unicode escape")
    n=0; for(i=1;i<=4;i++) { c=index("0123456789abcdef",tolower(substr(s,i,1)))-1; n=n*16+c }
    pos+=4; return n
}
function utf8(n) {
    if(n<128) return sprintf("%c",n)
    if(n<2048) return sprintf("%c%c",192+int(n/64),128+n%64)
    if(n<65536) return sprintf("%c%c%c",224+int(n/4096),128+int(n/64)%64,128+n%64)
    return sprintf("%c%c%c%c",240+int(n/262144),128+int(n/4096)%64,128+int(n/64)%64,128+n%64)
}
function string(    out,c,n,lo,b,length_,i,x) {
    if(substr(doc,pos++,1)!="\"") die("expected string")
    out=""
    while(pos<=length(doc)) {
        c=substr(doc,pos++,1)
        if(c=="\"") return out
        if(c=="\\") {
            c=substr(doc,pos++,1)
            if(c=="\"" || c=="\\" || c=="/") out=out c
            else if(c=="b") out=out sprintf("%c",8)
            else if(c=="f") out=out sprintf("%c",12)
            else if(c=="n") out=out "\n"
            else if(c=="r") out=out "\r"
            else if(c=="t") out=out "\t"
            else if(c=="u") {
                n=hex4()
                if(n>=55296 && n<=56319) {
                    if(substr(doc,pos,2)!="\\u") die("unpaired Unicode surrogate")
                    pos+=2; lo=hex4(); if(lo<56320 || lo>57343) die("unpaired Unicode surrogate")
                    n=65536+(n-55296)*1024+lo-56320
                } else if(n>=56320 && n<=57343) die("unpaired Unicode surrogate")
                out=out utf8(n)
            } else die("invalid string escape")
        } else {
            n=ordinal[c]
            if(n<32) die("unescaped control character")
            if(n<128) { out=out c; continue }
            if(n>=194 && n<=223) { length_=1; b=n-192 }
            else if(n>=224 && n<=239) { length_=2; b=n-224 }
            else if(n>=240 && n<=244) { length_=3; b=n-240 }
            else die("invalid UTF-8")
            for(i=1;i<=length_;i++) {
                x=substr(doc,pos++,1); n=ordinal[x]
                if(n<128 || n>191 || x=="") die("invalid UTF-8 continuation")
                c=c x; b=b*64+n-128
            }
            if((length_==1 && b<128) || (length_==2 && b<2048) ||
               (length_==3 && b<65536) || b>1114111 || (b>=55296 && b<=57343)) die("invalid UTF-8 code point")
            out=out c
        }
    }
    die("unterminated string")
}
function parse(    id,c,k,j,n,t,child) {
    ws(); id=++serial; c=substr(doc,pos,1)
    if(c=="\"") { type[id]="string"; value[id]=string(); return id }
    if(c=="{" || c=="[") {
        type[id]=(c=="{" ? "object" : "array"); pos++; ws()
        if(substr(doc,pos,1)==(c=="{" ? "}" : "]")) { pos++; return id }
        do {
            if(c=="{") {
                ws(); k=string(); ws(); if(substr(doc,pos++,1)!=":") die("expected colon")
                for(j=1;j<=count[id];j++) if(key[id,j]==k) die("duplicate object key")
            }
            child=parse(); n=++count[id]; children[id,n]=child; key[id,n]=k
            ws(); t=substr(doc,pos++,1)
            if(t==(c=="{" ? "}" : "]")) return id
            if(t!=",") die("expected comma or closing delimiter")
        } while(1)
    }
    t=substr(doc,pos)
    if(match(t,/^-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?/)) {
        type[id]="number"; value[id]=substr(t,1,RLENGTH); pos+=RLENGTH; return id
    }
    if(substr(t,1,4)=="true" || substr(t,1,4)=="null") {
        type[id]=substr(t,1,4); value[id]=type[id]; pos+=4; return id
    }
    if(substr(t,1,5)=="false") { type[id]="false"; value[id]="false"; pos+=5; return id }
    die("expected JSON value")
}
function quoted(s,    out,i,c,n) {
    out="\""; for(i=1;i<=length(s);i++) {
        c=substr(s,i,1); n=ordinal[c]
        if(c=="\"" || c=="\\") out=out "\\" c
        else if(n<32) out=out sprintf("\\u%04x",n)
        else out=out c
    }
    return out "\""
}
function render(id,    out,n,i,j,best,previous,first,k) {
    if(type[id]=="string") return quoted(value[id])
    if(type[id]!="object" && type[id]!="array") return value[id]
    out=(type[id]=="object" ? "{" : "["); first=1
    for(n=1;n<=count[id];n++) {
        if(type[id]=="object") {
            best=0
            for(j=1;j<=count[id];j++) if((first || ("x" key[id,j])>("x" previous)) && (!best || ("x" key[id,j])<("x" key[id,best]))) best=j
            i=best; previous=key[id,i]
        } else i=n
        if(!first) out=out ","; first=0
        if(type[id]=="object") out=out quoted(key[id,i]) ":"
        out=out render(children[id,i])
    }
    return out (type[id]=="object" ? "}" : "]")
}
function member(id,k,    j) { for(j=1;j<=count[id];j++) if(key[id,j]==k) return children[id,j]; return 0 }
function lookup(id,path,    names,n,i) {
    if(path=="") return id
    n=split(path,names,".")
    for(i=1;i<=n;i++) {
        if(type[id]=="array") {
            limit=sprintf("%.0f",count[id]-1)
            if(names[i] !~ /^(0|[1-9][0-9]*)$/ || count[id]<1 || length(names[i])>length(limit) ||
               (length(names[i])==length(limit) && ("x" names[i])>("x" limit))) die("array index is invalid: " path)
            id=children[id,names[i]+1]
        } else {
            if(type[id]!="object") die("path is not an object: " path)
            id=member(id,names[i]); if(!id) die("missing field: " path)
        }
    }
    return id
}
BEGIN {
    for(i=0;i<256;i++) ordinal[sprintf("%c",i)]=i
    if(mode=="quote") { doc=""; while((getline line)>0) doc=doc line "\n"; sub(/\n$/,"",doc); if(substr(doc,length(doc),1)!=sprintf("%c",1)) die("quote input sentinel missing"); print quoted(substr(doc,1,length(doc)-1)); exit }
    for(file=1;file<ARGC;file++) {
        doc=""; while((status=(getline line < ARGV[file]))>0) doc=doc line "\n"
        if(status<0) die("cannot read JSON file")
        if(ARGV[file]!="/dev/stdin") close(ARGV[file]); pos=1; roots[file]=parse(); ws()
        if(pos<=length(doc)) die("trailing content")
    }
    id=roots[1]; if(!id) die("missing input")
    if(mode=="merge") {
        other=roots[2]; if(type[id]!="object" || type[other]!="object" || ARGC!=3) die("merge requires two objects")
        for(i=1;i<=count[other];i++) {
            k=key[other,i]; index_=0
            for(j=1;j<=count[id];j++) if(key[id,j]==k) index_=j
            if(!index_) index_=++count[id]
            key[id,index_]=k; children[id,index_]=children[other,i]
        }
        print render(id); exit
    }
    id=lookup(id,path)
    if(mode=="type") { print type[id]; exit }
    if(mode=="length") {
        if(type[id]!="array" && type[id]!="object") die("length requires an array or object")
        print count[id]+0; exit
    }
    if(mode=="get") {
        if(type[id]=="object" || type[id]=="array" || type[id]=="null") die("expected non-null scalar")
        if(index(value[id],sprintf("%c",0))) die("NUL cannot enter shell values")
        printf "%s",value[id]; exit
    }
    print render(id)
    exit
}
