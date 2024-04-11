;; (define-module (rvim)
;;   #:export (print_msg))

(define p (make-soft-port
           (vector
            (lambda (c) (rs-send-str c))
            (lambda (s) (rs-send-str s))
            (lambda () (#f))
            (lambda () (#f))
            (lambda () (#f)))
           "w"))

(set-current-output-port p)
(set-current-error-port p)

(define curr-buf (make-parameter (rs-curr-buf)))
(define (curr-pos) (rs-curr-pos (curr-buf)))
(define (char-after) (rs-char-after (curr-buf) (curr-pos)))
(define (insert-str s) (rs-insert-str (curr-buf) (curr-pos) s))


(define (lorem-ipsum) "Lorem ipsum dolor sit amet, consectetur ...")

;; (object->string (current-buffer))
