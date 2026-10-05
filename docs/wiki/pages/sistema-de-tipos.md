# Sistema de tipos

O estudo original propõe tipagem estática, forte e nominal, assinaturas obrigatórias e
inferência local; tipos algébricos com `match` exaustivo; `T?` como única forma de nulo; `T ! E`
para erros; generics monomorfizados e traits; efeitos e contratos como extensões. A evidência
reforça quase tudo e acrescenta um requisito que a proposta não tinha: o tipo precisa ser
verificável enquanto o código ainda está sendo escrito.

## O que a evidência diz

- **Tipos são onde o modelo erra.** 94% dos erros de compilação em TypeScript gerado por LLMs
  são de tipo ([Mündler et al.](https://arxiv.org/abs/2504.09246), PLDI 2025). Tipagem
  estática não cria esses erros; ela os traz para a compilação, onde o agente os vê e corrige —
  ver [[compilador-semantico]].
- **Anotação sozinha não muda o resultado; `Any` piora.** No
  [MultiPL-E](https://arxiv.org/abs/2208.08227), tirar as anotações do Python não fez diferença
  (p=0,23), mas trocar os tipos das assinaturas do TypeScript por `Any` custou 2,5 pontos
  (p<0,001). O "nenhum `Any` fora de FFI" do estudo original está certo.
- **Inferência local recupera o custo em tokens.** Linguagens com inferência (Haskell, F#)
  ficaram perto das dinâmicas na comparação de
  [Alderson](https://martinalderson.com/posts/which-programming-languages-are-most-token-efficient/).
  "Explícito na fronteira, inferido no interior" é o ponto certo, e o MoonBit chegou à mesma
  regra com assinaturas obrigatórias no nível do módulo.
- **Restringir a geração por tipos funciona** — cortou 52–75% dos erros de compilação —, mas
  exige saber o tipo esperado no ponto em que o modelo está ([[decodificacao-restrita]]).

## Requisito novo: tipos verificáveis sobre prefixos

A proposta fala em "Hindley-Milner simplificada". Inferência global deixa um uso posterior
decidir o tipo de uma declaração anterior, e então o tipo de um prefixo do programa não está
determinado. Para o compilador checar a cada edição e para a restrição por tipos funcionar, a
inferência deve ser **local e bidirecional**: o tipo de cada declaração se fixa nela mesma
(pelo valor ou pela anotação), da esquerda para a direita, e as assinaturas obrigatórias dão o
contexto. O custo é exigir anotação em casos raros, como uma lista vazia sem uso imediato
(`var xs: [int] = []`).

## Mantido do estudo original

- Tipos algébricos: registros em uma linha e tipos soma; `match` exaustivo, com o compilador
  apontando todo `match` que precisa mudar quando um caso é adicionado.
- `T?` é o único tipo que admite ausência; usar um `T?` sem verificar é erro de compilação.
- `T ! E` é açúcar para `Result[T, E]`, com `?` para propagar e `fail` para falhar; pânico só em
  violação de invariante.
- Generics com colchetes, restrições por trait, monomorfização; `dyn Trait` só quando pedido.
- Nenhuma conversão implícita; `if` só aceita `bool` (e `T?` na variante A).

## Ajustes

- **Truthiness proibida, mas barata de evitar.** A regra impede as armadilhas de `0`, `""` e
  `[]` do Python, e custa 5 a 6 tokens por `len(xs) > 0` ([[custo-em-tokens]]). Uma
  `is_empty()` na biblioteca padrão reduz o custo sem reabrir a armadilha.
- **Opcional sem a semântica do `or` do Python.** Na variante A, `x or padrão` e `if x:` usam a
  sintaxe do Python com semântica diferente; a variante B usa `??` e `is not None` — ver
  [[sintaxe-da-linguagem-x]].
- **Campos posicionais em variantes**, como `Neg(Expr)`, além dos nomeados — o piloto mostrou
  que é o que modelos escrevem.
- **`int` é apelido de `i64`**, e literais inteiros são `int`.
- **Uma só semântica de overflow.** O estudo propõe erro em debug e "wrap ou trap,
  configurável" em release. Isso faz o mesmo programa se comportar de formas diferentes entre
  builds e contradiz a exigência de semântica idêntica entre destinos
  ([[estrategia-de-transpilacao]]). Recomendação: trap em todo build e todo destino, com
  `wrapping_add` e afins explícitos para quem quer aritmética modular.
- **Igualdade com erro nos testes** escrita como `== Err(E)`, não `== fail E`.

## Efeitos

O estudo recomenda marcar `uses io` na assinatura e começar só com `io`.

- **Não há evidência de que anotações de efeito ajudem LLMs ou ferramentas** — a busca não
  achou nenhum estudo.
- **A prática pesa contra efeitos tipados completos.** O Effekt descreve sistemas de efeito
  como "often complicated and potentially hinder a wide-spread adoption" e os modela como
  capacidades; o Unison infere as habilidades como a união das exigidas pelas funções chamadas;
  o OCaml 5 tem efeitos sem tipo; o Flix usa pureza para otimizar e paralelizar.
- **O Zig 0.15 tirou `async`/`await` e passa uma interface `Io` como parâmetro**, como já fazia
  com o alocador. A "cor" virou um parâmetro comum — é um efeito `io` expresso como
  capacidade, sem nenhuma feature nova no sistema de tipos.

Recomendação para a v3: começar por capacidade passada como parâmetro (estilo `Io` do Zig),
com inferência no interior e anotação só nas assinaturas públicas, e medir no harness se a
marcação muda alguma coisa para o modelo antes de investir num sistema de efeitos. O mesmo
mecanismo serve à [[concorrencia-sem-cor]].

## Contratos

As pré-condições `where`, verificadas em debug, seguem na v3. Nenhuma evidência foi levantada
sobre elas.
