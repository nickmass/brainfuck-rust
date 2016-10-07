cargo run --release -- programs/mandelbrot.bf | llc -O3 | gcc -O3 -x assembler -o mandelbrot - && ./mandelbrot
