from __future__ import annotations

import base64
import subprocess
import zlib

_PATCH = (
    'c-pl++jiSHlJEWsE}zH|$=Go&ZJpJeq#bvUx6^iy?ey-f+lPmiAe$44RCsZs&idcC3KtRp0g7_c?ioiSQH4TPp->k94#Ut%p5;kX'
    'r&&|QWs}$Giae{z`Lj<=@@J7JY23cOOKS2t**qipU7C|<z1f>gCeH2C!QZ@b!k57-XYw-m!Fls$&q-J7qO6_Uq$00gv|pF=yw2Kh'
    '?4K;X-J9r7qU3gNc#8dZ;6?lK-&K*fUy6#7bMP`aIB+HhuY-f<GD&}?>t&jeJ!e@KD`#G08JX8<kyqHKXmp%pS#q0^z&Wi+nbbwu'
    'w$E$w1&I+&`IA+WCwHXWo6u&9q)z5pQdOi9?b=5i6MQGgWAXmO)p5{16m7^nT_u@l@{WDwFV{(ZFVXQEeC2QI%{tBR#EZB2W-p}A'
    'n<ia|r}+5=r&fOc;lt^>fWLKGrmd5FkyH-+Z@u81wAxUY$%2%kWgI_{vI2g_F*a{o@uy5QM~6%{N3Vi|{TEa^_*0U)DRbwn$O-nn'
    '7cR)siK}Eu;*w-B4%owAQ>Us+-wFRt2R+>j5qJ(HrpP~MMjz9bK6{rgyVp@##mQ||WKB&x-ziGRjUv}cbE(NXDeH>krGzDEMVz0R'
    '+8=|!%Vb`klx0zRE;LP-@RiELfj2l(lv(90i}E&IEJ*HhI`{zFj`k*Oghf-%NxUi+O-5jvt%x?vx>;xB8omY2ZBb-5)4d6z%Zn1!'
    'A^npqV9W>+m1LbIbK<#w{NV<U`wTj!E$M)uQ3nxK>nyFkt_kocFKXTrxRT@>Pdvbpun)i{+CpI*XV+v5yI>IT{BGm0K|5^FjCsJa'
    'G=Bj47J{gZBuhFr=-@g$xZ$K$pbPVlFyG)EYcg}(I8O7lj$^P#Zabu|sYD-Vv|tLuK5fyH3;+>rMDj)TIR#yE5vgxSas9UrNkC!Y'
    '<Kaz<up-~Yp2}G@6<AZ^{I;p@i`+pAR%wz`NYVrdN}qxrf~KaV1g)3lDZ}}Hi4?ioyOA6HyGV0S=`oOGv^T;3sFif$ON#o(CSPRa'
    'r!)r}<0<Py^ed?ooayXkpspSf1AkwjaT3FFT^AoXi9_jwAuOEB^A8utC-ILT&fdL0VWw?GRz<mSQ1V4nGiGn9nlPJNMSuqBM2vL)'
    'a>es1k}U(yl9)nGokcpY+02DsH*_|meUtbMY^ca$>SSqEUu!d+LEWH+-E<I&y?{O{Lc73b@X_<TmMCX1ok^WO5J4HrT&6KfVPusj'
    'i(b-jv_^qC<4pESbmFT>KsBXSQqO{RwfPg(4smx=q{-7y4RJL})PAF6vG63yw8l&%QL<ix1@JrzD+EJ*UuXmE2$Ue=EJD-p)ymeX'
    '7zRp7=_8h9#fa&!JnDUMa)vwWcOnlY?mAMfrm<TM)mE`q<ZcDV2MLtG#0nJYqC)gIrN1EJGRg0VcYsDU_XHx}Hz*FLn2|K=tc?J='
    'o>HVYejiljBUKI=n-zjfIf7XSh9GIOT7iQV*fr<|&N`;D!`iWQ|5NzhA?qo;rPgrzm5yY(|6=j@ch|zaKo=3on-%)9#H+wel7**?'
    '!S@F=jEgnC-L%B_X%}y%!)n(eK&?^vAuN%dk_^rHV#x9_fH&*1Sd+5ebXHm-qM}2mwpbvL*FCmcAwHEuvdv!tkK<}h=AKGnAkC5{'
    '%lI@0b1vI^bnU<^2cmH)kY3eEc}Hr)OG+y|mB?I>jG_uSRQCoZu;`mQ1}&rcK(x6dd)+z{)X*!!0mryR##9icr)dE1HPENO!WY$p'
    'o3*62WQutx!6>|k!L~ZX*Ib3yOt5t+jk7K{=22@Jcr6e~t1bC5C+phzg>2|5gi#GV($UdX>y)yKEmi`P&wL61(Z_%hA*Ww;_is0~'
    'BBeg{@(i2VC+RifecDjYFWE3?6eSJ8$Um-F*AiJb;5^TZ1%x6^y$pYF6$y6Yx5^K_pZ=t@D~rxt+P4k4I+F~Rg)RZD1pA|I*62n@'
    'ZAVg5{O?U0?0dkuKS@5jDjW0{vZAi&XTNoGkD+N+++C1)Q7$-DbM`F$NH&yc_Oz;t5|@?~oJ9=N#ZxfptQE%0%p;{sd5oC^4?Nw5'
    'y?6ev%`Z?ch%Zwbls`XwMT7Fg!{@<47n650P+R+Oh;dnfeVIA!X9z$cvVMTTT5m3f#36JX&m5Pxs9b&Z5ujrH3JL`;U_q2t3?;b('
    'C%fFxRDu<SXcb{vX*;g0n138b7_}I&t*f&}WDo{;Bbrn_;IhdF;NWdok~vA&Hc<Xj9YBkea)Y(t+%VwiGeCxqS_EwHp<OSc)yGAO'
    'CZ|Myjn5hK1y~msAH}rUt{Y>}42j?g@M6HDK-qMAwI3Yp!*qLj6dWD)rkk5VFcp(8WZqzM5<aa9aL(PS^HY*lu);c>i{wb5D4MaJ'
    'igxY}jWJb|BfCC4lQ1x8B089Uw8#yk9Hko8+~hN;2<j0vx0N^4QAuDVm|6AcLXzfpGqIHTy%o))NdN2d{Olb;gG|@1!1<7;_)U7|'
    '53PCTpp%p52=bH!hhb&pOiZgZ2iu&_AvjJx2Q(2tvu5xO7Y@(mDN#W3A2a9Rdr>6u27po;Wwc$kHr5}amjeb=UHtD)SV<MHni|zn'
    '&vg)z<2pE)=Aezb<&n3=HPVt3!4Ym)P(+71?WHXYEZJ}&?wGU~$rSv7aRR-xFGXI+fL=nQX1Q*uHGQPF>rX?~mjD#VC=W>U6kMVK'
    '%r_0Cm~>a0wU}wYqn9x>!@l<{<6`pqy=amH*3m4QA&3V>z{b$R_>OUyM^Ing%d59PpPv1UdG(8vx9|StJ{qA=hSWRddTT-5vR|~7'
    ')K>-h3-zrflKQ*w!Sr4tj5h1O`}!I{r?p6%^{G~8x+f|3j7v#io^>AsCWjgQt@YUV%sCB8diHaf*2KIh2E(?<oUYyA;?>W8wDx@J'
    '9BAQgXLqO0b)PR^`;TNOI{T-b(nxw3Sm30&OC5iYgG!RKgQ5v(JELC>Z4Ii=Qq2g(@_{tP#JL;bte9oV>UNPhnB<w#-||>A$=(FU'
    'BoL<L9e|`I+~^D<uEiC>pgSoyKfxof7bcUL``Oh-pgR=W>vc(%>6cmOwugEP@bPAS_f>C1G6H(b{k1NYf%Xxwl4PY#Z0f@-n$_Cl'
    'Iu`JD;6mnAQ<6BT=4r|?QWn$6t9!=Gz9V2nRo`>}$X)aMW!6;pp7yS&qGh$o=bmVU*@VK=SAkkEUKCt;b1h^BY@#aJLIguDW|zc6'
    '?Mvq?Qr{N~X+G23TL(&Q$w=oSu!PohqN{BoTGnR1s^Oxa-jdXxr&ykp`!(2e*?RSi8cWSd9MGd4S{dinCoM@JmC4Py8)J0+Ou`f0'
    'lP{LulRKD2e?{+0H~Okl@EGy#u=M8iXpv|TlGQf|h}<ivt7nYU(F#ckj8Kv(jYwEjht7H*T<T-&`izP}s(G>|3#Yy(xRGR;-oZ3q'
    'IEb<DFt_2gX#%h}=J3y_Oi^8ap~w~0E~iy(X7X{6+{@X){RbMz9BZm#h=9GxlZPawn`z_`8S^EII``MCUdTeFl2$Q>-2Qi^Q=gb?'
    'y19luR?QN|@`t#e<%KhO`TAw>{78xAskI0CpuYkgab?zjF3OM98Z4u*plw56Z8*PAvxP|WbWkk8gDE^$RX<MxkTYw(AwQv6bq1+r'
    '>n!L@kXn+?fUcjUGea9`bu?ouTUn4IY6Y{}J=ca;D22#%-J?3E_#>f?L1af1M3Vl6CXedd^stArx~GMhd9mKWY7733u~U`Az}(0p'
    'RQNLNVycqr0!VkC|AdBL{O7>+Z3tySw2GrG3wqaVY1+mkjT?sk3`BWm%~f;5K69{Gn#$}^w@zJUX)q7~jEF33F@q)2*s)_vB(gmO'
    '*&dSUqV&gOVi5{oo<@K@4mY_OK=C;6PW8Vn4Q<2fhai1W&DiDZrWUh9Sp(=BJv~od@20lFM0MFzV0uFEcbZH<(`~#jp9cr~ubs*3'
    '7e~Rt%Puud3wQY)sp>X#{?x#?sl)x|Q~L6PJ!KW9cJtHQd~+iiW$J*?cG0>l>SA7GVuvWY9=~0jU!5PHzmI=Exwt$%Khw<lkWFnM'
    'Qd+NR^yvuHPMc}%ETLv@42yvvP+1f@qm|@Ep3ajj=ItaT;tk`LXdn<uM9^e9Zq*gA;z0+B($TEEOyx}e66ozT5@)?9l6KN6&JwLT'
    'L~O;H<X*puKUDn+5biuahG}^Pu@n7r@$SDb0%yN?wZAX9M2ctKvOrnALql*RTJDA2Yq|7fcIq*bFi7$-VjN~gDVy8hqW|8$KO)}Z'
    '{D*eIjODqct~3iA_VU6#$AIq8kCxbj$2$)A!}sUMzr-j11N}(f&wkXL6rLOY5rKilr|0TUgYYStOeQMSkR-#h5KXYc<h}!`RdRMX'
    ')Dq-vcsiy2H4o|e)*7Aer%c6NrsL=-8LF#5sQIAg88g{Uissa9J9tYRsh0sbhem{_Z6eTURVS7q#$A*uM(QN_LVwR=)|oA4-ATU2'
    '<yw*PK+xJJY@2_xn~^pozAc(z(25aL&hG6fX$!0<tX55pQ3@3Z9)d~I942TEb8}v-)|mT*IhxY#h{#~q;vn@MAKl*XpdC|lUb;IQ'
    '0*1pfNL{K@Zfkd`em52IEizP2v{-8LhM1^a(TJ!84V?5YFG>;<6u&Vcpqd9uOra+(?S+B1Kw}wIk1M$5+IiN;b7R}AYbgV9f^c7k'
    'b@>qanZ7}9YZ0rD3;gWy;(^ue;AlfPuv8IxiFzr{rpL0pej)fyvBFxJ%@A-)g*O9{!BDwauFRR*VX3YwmFj!QOnxk7ZV6P>@8AKa'
    '-D8#(0!3L4zP=jf(4xWJ-Eo`t?_v%$`oNt4UmsO?RL$F@p5H4aE5?uxx3zpFbq}-G;M{Z#0;f*rAFDu5<<j0``p%=DeXE1Pa}9LY'
    '=yiwrW*h|V!XF}0un<}1b&n$$8s(iii{yeI_K+?}NsESUNJk%85>GBm$VdR}!Ep09OXno7`e5RP1^jP+W8*TsejKb{tMke?MMK+6'
    '%4EQx46HZS3j1KtsuqK+D2ml8t!sK@kQNfh?H1{%<7ru1THP;RW0YYtFuTp{$#&J;I=hXlzdpllOW4!q{SIsPQ>?*|J8HKGW1+ZJ'
    '*h21B%V75!2d#r0%xWRFfsC*cRurA3n3&Uuj&=2^<`9^p82QN><P+7D1v_hrU)T|2UNq>G%=3E@^qN)$bfkUNQ&1c9w;wz9NT$w}'
    '+}~~?x@}Fsd>f`0oil#^Tk6?iJulMb5}NT7>-WUDD;`Lh;{h}l?$3Eo?x1D(M$Q}J#P7#_Q;ar70^c!e`>Q2k2}YAFi@Tr!cVc*b'
    'a}v>`2^KLtP!832MtSC}(%dpg`J@BN;@V}9_<(`CH>x5?q?vN=;kOJU93H|q5o^Q_ehT7Fz6|4rX5%rp^+%Qc;da({kmJ}tl<4Yk'
    'bz^)24Z|ZJq+pfg>5?V(4Zg3d*DYYh(aRPFms1q%B`E!h3=@>T6VMQ|^?_Q%^hij(S<9<p|D_3qZmj@;{%9DiQXxbd&;Zk$3R8Io'
    'MQK$m@$!n=^S3~w$fw=>*am4-+NnMZcErMHP;_`3hO+n^&kN8FERZ%UFf=t+>F&+I95$h>=J#ZkK$C}*>)Jm*oL`;9SEt9n#Fxka'
    'IQjK$d~x#osZCcuHA$A%8=e{ka9a%6)?|Nw`~LKuYYXHS%^JM0q-IP4pw$}nW8c<(UKUja%i^b|sL5bUwn#$@R*DS`bzH*?E>c{`'
    'b0)IyHAuSIaT9}3sMqUWUB;(pr&reYlIE^~22wr@4MbM9lnQ4o<fD0_;sOsWPH^OLQ_thP`0V)u%BrPMlqe1>%k7bMZk<A5M@?HM'
    '!h!7)0xqEE+4Fk#+z)zn^Fa%4r;Uv}6vl8z%K91lmc-t+Tm#qpwf~_((EYRQsIxShz)|<F45cpn#V%zFrr@)vfQau_uS{ByclXJ*'
    'EXZRj9To>=BrUr-DZ5d$J8yP#SjJGJI4%=gSiaqPF@uhAV8$J17?XH@nj>RIGs>CS25NI?CfkhrR;OkwhC#<>GPF|c<lKzHwAI0}'
    'D39jk*dy27+@IYxU+w4^nQ<7LqsjJ7Yr=Q1H?=!S1GQO$qcoH$Q=FyAE*szMPSbZ8yxn=S!wfo5cCax{)NZU9=S+<`OZ9C|)wX@8'
    ';~c9oE6uk!Slg)F&B+?M$#Tq~#yMQO*T}b)WzFT;k%#`YDs8V&J_v?ITW`z8Vz#qpxRJ4bKbtv2^?F5MSw)m{DE8(ynfu?g3E|0`'
    '2l^#2sm_KvYCZF{lM}v7b6E9EB|3gMq3w2R=YzT%W<bdY_LJ<o|K!{EpirhZCEbQO8`|4Gv=^nX3Onp~8?#eoo2K6k?H95r86UN@'
    'Q>s-1r&qgmcHPnW;Y{0tWk${|<2)-WBJlL<OUinn@7hzU$eqrF!+N6Xr%6?uZhyMC)>AOivQzzc)(l*~zh<{O?xTObzaRYIOpab0'
    '@++}rHnBR-2<g!Rt*AYS8Wwf)c$ru!NmVn7**EXW_4u7$fpfuPIR0(RH4gnf!mtj5MB0rKUtPRj1>Lpdw^wi9pZ^@cgMZ_HemJ@K'
    'SNzl4)AuLu@X~|@slnso$3FnG|7f{ZVka_9JN}+P-=n{$n^MZ(>E4d0Ss+0>`1SPi5_ds8iaUQ^AH5C^_npbX{)_$KaPW?;Zh?@i'
    '&g${^_T>9JacodaTZx10pZ*NR&8h0XGv(T&4pu+3sOFB?!9M-GV%Rsa;@VkyLwe5cA>?F4<_l<hs<t4w?~l$@yvBxY>DO)q@2;L5'
    '0_QvU$6U}fh(>Z@`MCPNv!0IIR1rI>nnQe1d;0;qlmR#U$@kLGYm9OKp5f+WBjc14!k-!tMtgx>KljsN;2)SB=9jQeK`##c>wWz&'
    'R13upC~Sp!>>bk8DT8RJYHsmP9yZ`ezL;}|Q@s6o-(3&Pw<H6>-39jbJ-;HDAN9a2PP!np;LR`FWw)!k);mTJCB)|RB3r&Kz%T|7'
    'a}A3F`>O&T3ZIYMS5Gjv*uKx0)hm%ZQmM^I^7iNiyXes2X*}NEK4wZkSngQUU^Z}hMD10f{IVIg4A1Y0a-(a1gV`n}*@9o4iMgap'
    'KB8kPK+m)rO;X^QS+7m!Ryz%znbg_}G7~O3h&83R=f~`Yr=e*b4LvwfXSu<HPnTl)qmh(ZZrY8fh@>Q|;(>TQc?dh)yQo>#TlLmU'
    'q#ykn!VICUYd(+H#ag`?8y!9U(Bhy{j5v?9O*zcRpGPT(`B3y$B^dyFlA?rL_Mo&16Pkn}tAMnq^xDb>%D$O*^cpb?J-k*Jw&8}R'
    '7)?`Lc?6cT_G7fubcaJ5KA1$u{$k0&%Y)zuJh#K6AA+Nv<0gJO7uEc}^II(q_#dZ<eoPkso38OjBa?#IZ&>rEX1u!JYUUsQNFMF^'
    '*-z&ezrH;?K8Y_*etZ9R*qI+V-97^4qnYkjU_wGtR<QV7dH9Q7myHV&SPJ+e{RM?6if+1VFSQNBphM<AnN-8Ptmtp>F`=W|Wd5<H'
    'v7hzNBn^SEyLAsyd)PmyJUA5ETm<_MZA6L~7I+^k-8da%e1|t-7R#6(*~U;<B!|3uwU&-IO1G8q^e6s#CM*DRdbv+=LZ7$5++>S$'
    'a1b#-*nu}5Vk>S++Vqq-YuhQ1K%-wG;i4OKSJE+s%P%3XO^nE9Gu*y<6*BLb|H&!!q_(9ijsgD)6aJkh34-2HMen#VLoz^p&<ZId'
    'hM^Z@rHfA$)WcVuDp-_i7@#~lI*Q@{Y9oeJ3RDcaf8P|f{6D6ZTpa'
)

completed = subprocess.run(
    ["patch", "-p1", "--batch", "--forward"],
    input=zlib.decompress(base64.b85decode(_PATCH)),
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    check=False,
)
print(completed.stdout.decode("utf-8", errors="replace"), end="")
if completed.returncode != 0:
    raise SystemExit(completed.returncode)
print("CodeQL path hardening patch applied.")
