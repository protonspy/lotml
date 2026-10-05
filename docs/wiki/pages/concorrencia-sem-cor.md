# Concorrência sem cor

O estudo original propõe concorrência sem marcar funções como `async` nem exigir `await`, com
tarefas em green threads, como em Go e no BAML, para eliminar o "esqueceu o `await`" e impedir
que `async` se espalhe pelo código. A ideia se sustenta; a evidência mostra onde runtimes sem
cor vazam e o que ela exige do [[modelo-de-memoria]].

## Precedentes

- **O argumento:** Bob Nystrom, *What Color is Your Function?* (2015) — Go "eliminated the
  distinction between synchronous and asynchronous code".
- **Go:** goroutines preemptivas desde a 1.14, ao custo de mais erros `EINTR` em Unix.
- **Java, threads virtuais (JDK 21, JEP 444):** a thread virtual fica presa à thread do sistema
  dentro de `synchronized` e em chamadas nativas; o JDK 24 (JEP 491) removeu o caso
  `synchronized`, e a inicialização de classes ainda prende.
- **Zig:** a 0.15 anunciou que "there will not be async/await keywords"; o I/O passa por uma
  interface `Io` recebida como parâmetro, como o alocador. Na 0.16 (abril de 2026) a
  implementação com threads está completa e a com eventos ainda é experimental. Passar `Io` como
  parâmetro transforma a "cor" num parâmetro — ver [[sistema-de-tipos]].
- **OCaml 5:** efeitos sem tipo, com cerca de 1% de custo médio para código que não os usa.
- **BAML:** green threads e "colorless concurrency like Go", segundo o README.

## Onde vaza

Runtimes sem cor vazam na fronteira com código que bloqueia sem avisar: chamadas nativas e
FFI (as threads virtuais do Java), extensões C que não cedem a vez (o gevent no Python), sinais
(o `EINTR` do Go). A linguagem X vai chamar Python e C, então precisa de um mecanismo para
entregar chamadas bloqueantes de FFI a threads dedicadas, como o Go faz com syscalls.

## O que exige da memória

Green threads que migram entre threads do sistema obrigam a contagem de referências a ser
atômica ou enviesada, e contagem atômica custou até 59% no Perceus ([[modelo-de-memoria]]). A
regra do estudo original resolve: dados enviados entre tarefas são copiados ou movidos, então a
contagem dentro de uma tarefa pode ser não atômica. Concorrência estruturada (grupos de tarefas
com escopo) mantém essa fronteira explícita.

## No destino Python

- **gevent:** greenlets cooperativos numa thread só; extensões C que bloqueiam não cedem a vez.
- **CPython sem GIL:** suportado desde o 3.14 (PEP 779), com custo de cerca de 1% a 10% em código
  de uma thread conforme a plataforma; o 3.15 sai em 9 de outubro de 2026 ainda sem ser o
  padrão.

O destino Python ([[estrategia-de-transpilacao]]) não reproduz as green threads com fidelidade;
como a concorrência é v2, basta mapear tarefas para threads e documentar as diferenças.

## Hipótese não medida

Nenhuma fonte encontrada mede a frequência de "esqueceu o `await`" ou de `async` espalhado em
código gerado por LLMs. O argumento do estudo original é plausível, mas é hipótese: entra no
[[harness-de-avaliacao]] como pergunta, não como fato.
