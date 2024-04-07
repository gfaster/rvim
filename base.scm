;; (define-module (rvim)
;;   #:export (print_msg))

(define p (make-soft-port
           (vector
            (lambda (c) (send-str c))
            (lambda (s) (send-str s))
            (lambda () (#f))
            (lambda () (#f))
            (lambda () (#f)))
           "w"))

(set-current-output-port p)
(set-current-error-port p)
